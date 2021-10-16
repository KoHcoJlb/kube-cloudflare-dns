use std::collections::HashMap;
use std::fmt::Debug;
use std::iter;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;
use tokio::time::sleep;

use kube_cloudflare_dns::api::CfApi;
use kube_cloudflare_dns::plan::{compute_records, plan};
use kube_cloudflare_dns::resource::{ResourceKey, WatchedResource};

async fn watcher<T>(client: kube::Client, watched_resources: Arc<Mutex<HashMap<ResourceKey, WatchedResource>>>,
                    changed: Sender<()>)
    where T: kube::Resource + Clone + DeserializeOwned + Debug + Send + 'static,
          <T as kube::Resource>::DynamicType: Default,
          WatchedResource: From<T> {
    let api = kube::Api::<T>::all(client);
    let stream = kube_runtime::watcher(api, ListParams::default());
    let mut stream = Box::pin(stream);
    loop {
        use kube_runtime::watcher::Event::*;

        #[allow(unused_must_use)]
        match stream.try_next().await {
            Ok(Some(event)) => match event {
                Restarted(resources) => {
                    let mut watched_resources = watched_resources.lock().await;
                    for res in resources {
                        let key = ResourceKey::from(&res);
                        watched_resources.insert(key, res.into());
                    }
                    changed.try_send(());
                }
                Applied(resource) => {
                    let mut watched_resources = watched_resources.lock().await;
                    watched_resources.insert(ResourceKey::from(&resource), resource.into());
                    changed.try_send(());
                }
                Deleted(resource) => {
                    let mut watched_resources = watched_resources.lock().await;
                    watched_resources.remove(&ResourceKey::from(&resource));
                    changed.try_send(());
                }
            }
            Ok(None) => {}
            Err(err) => {
                println!("watch error: {}", err);
                sleep(Duration::from_secs(30)).await;
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let zone_name = std::env::var("ZONE_NAME").expect("ZONE_NAME environment variable not set");
    let cf_token = std::env::var("CF_TOKEN").expect("CF_TOKEN environment variable not set");

    let kube_client = kube::Client::try_default().await.unwrap();
    let cf_client = CfApi::new(&cf_token);

    let resources = Arc::new(Mutex::new(HashMap::<ResourceKey, WatchedResource>::new()));
    let (tx, mut rx) = channel(10);

    tokio::task::spawn(watcher::<Service>(kube_client.clone(), resources.clone(), tx.clone()));
    tokio::task::spawn(watcher::<Ingress>(kube_client.clone(), resources.clone(), tx.clone()));

    rx.recv().await;
    rx.recv().await;

    loop {
        let expected: Vec<_> = {
            let resources = resources.lock().await;
            println!("Resources: {:?}", resources.keys());
            compute_records(resources.values().collect())
                .into_iter()
                .filter(|r| r.name.ends_with(&zone_name))
                .collect()
        };
        println!("Expected: {:?}", expected);

        if let Err(err) = async {
            let zone = cf_client.zones().await?
                .into_iter()
                .find(|z| z.name == zone_name)
                .ok_or(anyhow!("zone not found"))?;
            let actual = cf_client.records(&zone.id).await?;
            println!("Actual: {:?}", actual);

            let plan = plan(&expected, &actual);
            println!("Plan: {:?}", plan);

            for change in plan {
                use kube_cloudflare_dns::plan::PlanAction::*;

                match change {
                    Add(record) => cf_client.create_record(&zone.id, &record).await?,
                    Delete(record) => cf_client.delete_record(&zone.id, &record.id).await?,
                    Update(record) => cf_client.update_record(&zone.id, &record).await?
                }
            }

            Ok(()) as anyhow::Result<()>
        }.await {
            println!("{}", err)
        }

        println!("{}", iter::repeat("=").take(64).collect::<String>());

        tokio::select! {
            _ = sleep(Duration::from_secs(60)) => {}
            _ = rx.recv() => {}
        }
    }
}
