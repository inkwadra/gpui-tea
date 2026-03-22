use crate::observability::{Observability, RuntimeEvent, TelemetryEvent};
use crate::{Dispatcher, Key, SubHandle, SubscriptionContext, Subscriptions};
use gpui::Context;
use std::{collections::HashMap, sync::Arc};

/// Internal state for an active subscription.
struct ActiveSubscription {
    handle: SubHandle,
    label: Option<Arc<str>>,
}

/// Internal registry for subscriptions.
#[derive(Default)]
pub(crate) struct SubscriptionRegistry {
    active: HashMap<Key, ActiveSubscription>,
}

/// Internal stats for a subscription reconcile pass.
pub(crate) struct SubscriptionReconcileStats {
    pub(crate) active: usize,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) retained: usize,
}

impl SubscriptionRegistry {
    pub(crate) fn reconcile<Msg: Send + 'static, T: 'static>(
        &mut self,
        subscriptions: Subscriptions<Msg>,
        dispatcher: &Dispatcher<Msg>,
        observability: &Observability<Msg>,
        cx: &mut Context<'_, T>,
    ) -> SubscriptionReconcileStats {
        let mut current_active = std::mem::take(&mut self.active);
        let mut next_active = HashMap::with_capacity(subscriptions.len());
        let mut pending_builds = Vec::new();
        let mut added = 0;
        let mut retained = 0;

        for subscription in subscriptions {
            let key = subscription.key;
            let label = subscription.label;

            if let Some(active) = current_active.remove(&key) {
                retained += 1;
                observability.observe_runtime(RuntimeEvent::SubscriptionRetained {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                observability.observe_telemetry(TelemetryEvent::SubscriptionRetained {
                    key: &key,
                    key_description: observability.describe_key_value(&key),
                    label: label.as_deref(),
                });
                next_active.insert(
                    key,
                    ActiveSubscription {
                        handle: active.handle,
                        label,
                    },
                );
            } else {
                added += 1;
                pending_builds.push((key, label, subscription.builder));
            }
        }

        let removed = current_active.len();
        for (key, active) in current_active {
            observability.observe_runtime(RuntimeEvent::SubscriptionRemoved {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: active.label.as_deref(),
            });
            observability.observe_telemetry(TelemetryEvent::SubscriptionRemoved {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: active.label.as_deref(),
            });
        }

        for (key, label, builder) in pending_builds {
            let mut subscription_context = SubscriptionContext::new(cx, dispatcher.clone());
            let handle = builder(&mut subscription_context);
            observability.observe_runtime(RuntimeEvent::SubscriptionBuilt {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: label.as_deref(),
            });
            observability.observe_telemetry(TelemetryEvent::SubscriptionBuilt {
                key: &key,
                key_description: observability.describe_key_value(&key),
                label: label.as_deref(),
            });
            next_active.insert(key, ActiveSubscription { handle, label });
        }

        self.active = next_active;

        SubscriptionReconcileStats {
            active: self.active.len(),
            added,
            removed,
            retained,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.active.len()
    }
}
