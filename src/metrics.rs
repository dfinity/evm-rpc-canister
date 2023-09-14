#[macro_export]
macro_rules! inc_metric {
    ($metric:ident) => {{
        $crate::METRICS.with(|m| m.borrow_mut().$metric += 1);
    }};
}

#[macro_export]
macro_rules! inc_metric_entry {
    ($metric:ident, $entry:expr) => {{
        $crate::METRICS.with(|m| {
            m.borrow_mut()
                .$metric
                .entry($entry.clone())
                .and_modify(|counter| *counter += 1)
                .or_insert(1);
        });
    }};
}

#[macro_export]
macro_rules! add_metric {
    ($metric:ident, $value:expr) => {{
        $crate::METRICS.with(|m| m.borrow_mut().$metric += $value);
    }};
}

#[macro_export]
macro_rules! get_metric {
    ($metric:ident) => {{
        $crate::METRICS.with(|m| m.borrow().$metric)
    }};
}

pub fn encode_metrics(w: &mut ic_metrics_encoder::MetricsEncoder<Vec<u8>>) -> std::io::Result<()> {
    w.encode_gauge(
        "canister_version",
        ic_cdk::api::canister_version() as f64,
        "Canister version.",
    )?;
    w.encode_gauge(
        "stable_memory_pages",
        ic_cdk::api::stable::stable64_size() as f64,
        "Size of the stable memory allocated by this canister measured in 64K Wasm pages.",
    )?;
    w.encode_counter(
        "requests",
        get_metric!(requests) as f64,
        "Number of request() calls.",
    )?;
    w.encode_counter(
        "request_cycles_charged",
        get_metric!(request_cycles_charged) as f64,
        "Cycles charged by request() calls.",
    )?;
    w.encode_counter(
        "request_cycles_refunded",
        get_metric!(request_cycles_refunded) as f64,
        "Cycles refunded by request() calls.",
    )?;
    crate::METRICS.with(|m| {
        m.borrow()
            .host_requests
            .iter()
            .map(|(k, v)| {
                w.counter_vec(
                    "json_rpc_host_requests",
                    "Number of request() calls to a service host.",
                )
                .and_then(|m| m.value(&[("host", k)], *v as f64))
                .and(Ok(()))
            })
            .find(|e| e.is_err())
            .unwrap_or(Ok(()))
    })?;
    Ok(())
}
