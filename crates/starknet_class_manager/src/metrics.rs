use starknet_sequencer_metrics::metrics::LabeledMetricCounter;
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};
use strum::VariantNames;

const CAIRO_CLASS_TYPE_LABEL: &str = "class_type";

#[derive(strum_macros::EnumVariantNames, strum_macros::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum CairoClassType {
    Regular,
    Deprecated,
}

generate_permutation_labels! {
    CAIRO_CLASS_TYPE_LABELS,
    (CAIRO_CLASS_TYPE_LABEL, CairoClassType),
}

define_metrics!(
    ClassManager => {
        LabeledMetricCounter {
            N_CLASSES,
            "class_manager_n_classes", "Number of classes, by label (regular, deprecated)",
            init = 0 ,
            labels = CAIRO_CLASS_TYPE_LABELS
        },
    },
);

pub(crate) fn increment_n_classes(cls_type: CairoClassType) {
    N_CLASSES.increment(1, &[(CAIRO_CLASS_TYPE_LABEL, cls_type.into())]);
}

pub(crate) fn register_metrics() {
    N_CLASSES.register();
}
