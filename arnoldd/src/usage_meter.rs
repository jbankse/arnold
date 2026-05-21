use anyhow::Result;
use arnold_wire::DaemonEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Per-session running cost in USD, accumulated from cpu `usage` up-frames.
/// Resets when the session is closed.
#[derive(Clone)]
pub struct UsageMeter {
    inner: Arc<Mutex<HashMap<Uuid, f64>>>,
}

impl Default for UsageMeter {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }
}

pub struct TurnCost {
    pub session_cost_usd: f64,
    pub last_turn_cost_usd: f64,
    pub last_turn_provider: String,
    pub last_turn_model: String,
}

impl UsageMeter {
    /// Record a turn's cost (preferring `actual_cost_usd` over `estimated_cost_usd`)
    /// and return the new totals for the calling session, or `None` if no cost
    /// number was available (e.g. local Ollama with no override).
    pub fn record(
        &self,
        session_id: Uuid,
        provider: String,
        model: String,
        estimated: Option<f64>,
        actual: Option<f64>,
    ) -> Result<Option<TurnCost>> {
        let last_turn = actual.or(estimated).unwrap_or(0.0);
        if last_turn <= 0.0 && actual.is_none() && estimated.is_none() {
            return Ok(None);
        }
        let mut map = self.inner.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        let cumulative = map.entry(session_id).or_insert(0.0);
        *cumulative += last_turn;
        Ok(Some(TurnCost {
            session_cost_usd: *cumulative,
            last_turn_cost_usd: last_turn,
            last_turn_provider: provider,
            last_turn_model: model,
        }))
    }

    /// Drop a session's accumulator. Called on CloseSession.
    pub fn forget(&self, session_id: Uuid) -> Result<()> {
        let mut map = self.inner.lock().map_err(|e| anyhow::anyhow!("mutex poisoned: {e}"))?;
        map.remove(&session_id);
        Ok(())
    }
}

/// Convert a TurnCost into a CostUpdate event the session loop can ship to the client.
pub fn cost_event(session_id: Uuid, tc: &TurnCost) -> DaemonEvent {
    DaemonEvent::CostUpdate {
        session_id,
        session_cost_usd: tc.session_cost_usd,
        last_turn_cost_usd: tc.last_turn_cost_usd,
        last_turn_provider: tc.last_turn_provider.clone(),
        last_turn_model: tc.last_turn_model.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_across_turns() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t1 = m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.01), None).unwrap().unwrap();
        assert_eq!(t1.last_turn_cost_usd, 0.01);
        assert_eq!(t1.session_cost_usd, 0.01);
        let t2 = m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.02), None).unwrap().unwrap();
        assert_eq!(t2.last_turn_cost_usd, 0.02);
        assert!((t2.session_cost_usd - 0.03).abs() < 1e-9);
    }

    #[test]
    fn prefers_actual_over_estimated() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t = m.record(sid, "xai".into(), "grok-4.3".into(), Some(0.05), Some(0.0421)).unwrap().unwrap();
        assert!((t.last_turn_cost_usd - 0.0421).abs() < 1e-9);
    }

    #[test]
    fn no_cost_returns_none() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        let t = m.record(sid, "ollama".into(), "llama3".into(), None, None).unwrap();
        assert!(t.is_none(), "no cost number means no event emitted");
    }

    #[test]
    fn forget_drops_session() {
        let m = UsageMeter::default();
        let sid = Uuid::new_v4();
        m.record(sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.01), None).unwrap();
        m.forget(sid).unwrap();
        // After forget, the next record on a fresh sid starts at zero
        let new_sid = Uuid::new_v4();
        let t = m.record(new_sid, "anthropic".into(), "sonnet-4-6".into(), Some(0.02), None).unwrap().unwrap();
        assert_eq!(t.session_cost_usd, 0.02);
    }
}
