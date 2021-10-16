use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;

use k8s_openapi::api::core::v1::{LoadBalancerStatus, Service, ServiceSpec};
use k8s_openapi::api::networking::v1::{Ingress, IngressSpec, IngressStatus};

use crate::{APP_NAME, HOSTNAME_LABEL};
use crate::api::Record;
use crate::resource::WatchedResource;

#[derive(Debug)]
pub enum PlanAction {
    Add(Record),
    Delete(Record),
    Update(Record),
}

fn ingress_addresses(ingress: &Ingress) -> Vec<String> {
    if let Some(IngressStatus {
                    load_balancer:
                    Some(LoadBalancerStatus {
                             ingress: Some(ingress)
                         })
                }) = &ingress.status {
        ingress.into_iter()
            .filter_map(|i| i.ip.clone())
            .collect()
    } else {
        Vec::new()
    }
}

fn service_addresses(service: &Service) -> Vec<String> {
    if let Some(ServiceSpec {
                    cluster_ips: Some(addresses), ..
                }) = &service.spec {
        addresses.clone()
    } else {
        Vec::new()
    }
}

fn records_for_hostname(hostname: &str, addresses: &[String]) -> Vec<Record> {
    if addresses.is_empty() {
        return Vec::new();
    }

    let mut records = vec![];
    for addr in addresses {
        let _type = match IpAddr::from_str(&addr) {
            Ok(IpAddr::V4(_)) => "A",
            Ok(IpAddr::V6(_)) => "AAAA",
            Err(_) => continue
        }.to_string();
        records.push(Record {
            _type,
            name: hostname.into(),
            content: addr.clone(),
            id: "".into(),
        });
    }
    records.push(Record {
        _type: "TXT".into(),
        name: hostname.into(),
        content: APP_NAME.into(),
        id: "".into(),
    });
    records
}

pub fn compute_records(resources: Vec<&WatchedResource>) -> Vec<Record> {
    let mut records = Vec::new();
    for resource in resources {
        match resource {
            WatchedResource::Ingress(ingress) => {
                if let Some(IngressSpec {
                                rules: Some(rules), ..
                            }) = &ingress.spec {
                    for rule in rules {
                        records.extend(records_for_hostname(rule.host.as_ref().unwrap(),
                                                            &ingress_addresses(ingress)));
                    }
                }
            }
            WatchedResource::Service(service) => {
                if let Some(annotations) = &service.metadata.annotations {
                    if let Some(hostname) = annotations.get(HOSTNAME_LABEL) {
                        records.extend(records_for_hostname(hostname, &service_addresses(service)));
                    }
                }
            }
        }
    }
    records
}

pub fn plan(expected: &[Record], actual: &[Record]) -> Vec<PlanAction> {
    fn find<'a>(records: &'a [Record], record: &Record) -> Option<&'a Record> {
        records.into_iter()
            .find(|r| r._type == record._type &&
                r.name == record.name)
    }

    let managed: HashSet<String> = actual.iter()
        .filter(|r| r._type == "TXT" && r.content == APP_NAME)
        .map(|r| r.name.clone())
        .collect();
    let not_managed: HashSet<String> = actual.iter()
        .filter(|r| !managed.contains(&r.name))
        .map(|r| r.name.clone())
        .collect();

    let mut plan = Vec::new();

    for record in expected {
        if let Some(existing) = find(actual, record) {
            if !managed.contains(&record.name) {
                println!("Skip updating record {} {} not managed by us", &record._type, &record.name);
                continue;
            }

            if record.content != existing.content {
                plan.push(PlanAction::Update(record.clone()));
            }
        } else {
            if not_managed.contains(&record.name) {
                println!("Skip creating record {} {} not managed by us", &record._type, &record.name);
                continue;
            }

            plan.push(PlanAction::Add(record.clone()));
        }
    }

    for record in actual {
        if managed.contains(&record.name) && find(expected, record).is_none() {
            plan.push(PlanAction::Delete(record.clone()))
        }
    }

    plan
}
