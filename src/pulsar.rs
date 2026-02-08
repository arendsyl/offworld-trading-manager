use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use pulsar::{Pulsar, TokioExecutor};
use tracing::{info, error};

use crate::config::PulsarConfig;
use crate::models::NotifyMessage;

pub struct PulsarManager {
    pub client: Pulsar<TokioExecutor>,
    pub config: PulsarConfig,
    producers: Mutex<HashMap<String, Arc<Mutex<pulsar::producer::Producer<TokioExecutor>>>>>,
}

impl std::fmt::Debug for PulsarManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PulsarManager")
            .field("config", &self.config)
            .finish()
    }
}

impl PulsarManager {
    pub async fn new(config: PulsarConfig) -> Result<Self, pulsar::Error> {
        let client = Pulsar::builder(&config.url, TokioExecutor)
            .build()
            .await?;
        info!(url = %config.url, "Connected to Pulsar");
        Ok(Self {
            client,
            config,
            producers: Mutex::new(HashMap::new()),
        })
    }

    pub fn topic(&self, prefix: &str, player_id: &str) -> String {
        format!(
            "persistent://{}/{}/mass-driver.{}.{}",
            self.config.tenant, self.config.namespace, prefix, player_id
        )
    }

    pub async fn send_notification(&self, player_id: &str, msg: &NotifyMessage) {
        let topic = self.topic("receive", player_id);
        let payload = match serde_json::to_vec(msg) {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "Failed to serialize notification");
                return;
            }
        };

        let mut producers = self.producers.lock().await;
        let producer = if let Some(p) = producers.get(&topic) {
            p.clone()
        } else {
            match self.client.producer().with_topic(&topic).build().await {
                Ok(p) => {
                    let p = Arc::new(Mutex::new(p));
                    producers.insert(topic.clone(), p.clone());
                    p
                }
                Err(e) => {
                    error!(error = %e, topic = %topic, "Failed to create producer");
                    return;
                }
            }
        };
        drop(producers);

        let mut prod = producer.lock().await;
        if let Err(e) = prod.send_non_blocking(payload).await {
            error!(error = %e, topic = %topic, "Failed to send notification");
        }
    }

    pub async fn create_send_consumer(
        &self,
        player_id: &str,
    ) -> Result<pulsar::consumer::Consumer<Vec<u8>, TokioExecutor>, pulsar::Error> {
        let topic = self.topic("send", player_id);
        let subscription = format!("otm-mass-driver-send-{}", player_id);
        self.client
            .consumer()
            .with_topic(&topic)
            .with_subscription(&subscription)
            .with_subscription_type(pulsar::SubType::Exclusive)
            .build()
            .await
    }
}
