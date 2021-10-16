use std::convert::TryInto;

use reqwest::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub struct CfApi {
    client: reqwest::Client,
}

pub const CF_ENDPOINT: &str = "https://api.cloudflare.com/client/v4";

#[derive(Deserialize, Debug)]
pub struct Zone {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Record {
    pub id: String,
    #[serde(rename = "type")]
    pub _type: String,
    pub name: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
struct CfResponse<T> {
    success: bool,
    result: Option<T>,
    errors: Value,
}

impl<T> CfResponse<T> {
    fn result(self) -> Result<T> {
        if !self.success {
            Err(CfError::Api(serde_json::to_string(&self.errors).unwrap()))
        } else {
            Ok(self.result.unwrap())
        }
    }
}

#[derive(Error, Debug)]
pub enum CfError {
    #[error("cf api error: {0}")]
    Api(String),
    #[error("cf transport error: {0}")]
    Transport(#[from] reqwest::Error),
}

type Result<T> = std::result::Result<T, CfError>;

impl CfApi {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, format!("Bearer {}", token).try_into().unwrap());

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        Self {
            client
        }
    }

    pub async fn zones(&self) -> Result<Vec<Zone>> {
        let resp: CfResponse<Vec<Zone>> = self.client.get(format!("{}/zones", CF_ENDPOINT))
            .send()
            .await?
            .json()
            .await?;
        resp.result()
    }

    pub async fn records(&self, zone_id: &str) -> Result<Vec<Record>> {
        let resp: CfResponse<Vec<Record>> = self.client.get(format!("{}/zones/{}/dns_records",
                                                                    CF_ENDPOINT, zone_id))
            .send()
            .await?
            .json()
            .await?;
        resp.result()
    }

    pub async fn create_record(&self, zone_id: &str, record: &Record) -> Result<()> {
        let resp: CfResponse<Value> = self.client.post(format!("{}/zones/{}/dns_records",
                                                               CF_ENDPOINT, zone_id))
            .json(&record)
            .send()
            .await?
            .json()
            .await?;
        resp.result()?;
        Ok(())
    }

    pub async fn delete_record(&self, zone_id: &str, record_id: &str) -> Result<()> {
        let resp: CfResponse<Value> = self.client.delete(format!("{}/zones/{}/dns_records/{}",
                                                                 CF_ENDPOINT, zone_id, record_id))
            .send()
            .await?
            .json()
            .await?;
        resp.result()?;
        Ok(())
    }

    pub async fn update_record(&self, zone_id: &str, record: &Record) -> Result<()> {
        let resp: CfResponse<Value> = self.client.put(format!("{}/zones/{}/dns_records/{}",
                                                              CF_ENDPOINT, zone_id, &record.id))
            .json(record)
            .send()
            .await?
            .json()
            .await?;
        resp.result()?;
        Ok(())
    }
}
