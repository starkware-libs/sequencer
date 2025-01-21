pub struct MetricCounter {
    pub name: &'static str,
    pub description: &'static str,
    pub initial_value: u64,
}

pub struct MetricGauge {
    pub name: &'static str,
    pub description: &'static str,
}
