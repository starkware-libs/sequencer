use starknet_sequencer_metrics::metrics::LabeledMetricHistogram;
use starknet_sequencer_metrics::{define_metrics, generate_permutation_labels};
use starknet_sierra_multicompile_types::SerializedClass;
use strum::VariantNames;

const CLASS_OBJECT_TYPE_LABEL: &str = "class_object_type";

#[derive(
    Debug, strum_macros::Display, strum_macros::EnumVariantNames, strum_macros::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum ClassObjectType {
    Sierra,
    Casm,
    DeprecatedCasm,
}

generate_permutation_labels! {
    CLASS_OBJECT_TYPE_LABELS,
    (CLASS_OBJECT_TYPE_LABEL, ClassObjectType),
}

define_metrics!(
    ClassManager => {
        LabeledMetricHistogram {
            CLASS_SIZES,
            "class_manager_class_sizes",
            "Size of the classes in bytes, labeled by type (sierra, casm, deprecated casm)",
            labels = CLASS_OBJECT_TYPE_LABELS
        },
    },
);

pub(crate) fn record_class_size<T>(class_type: ClassObjectType, class: &SerializedClass<T>) {
    let class_size = class.size().unwrap_or_else(|_| {
        panic!("Illegally formatted {} class, should not have gotten into the system.", class_type)
    });
    let class_size = u16::try_from(class_size).unwrap_or_else(|_| {
        panic!(
            "{} class size {} is bigger than what is allowed,
            should not have gotten into the system.",
            class_type, class_size
        )
    });

    CLASS_SIZES.record(class_size, &[(CLASS_OBJECT_TYPE_LABEL, class_type.into())]);
}
