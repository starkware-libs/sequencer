use apollo_compile_to_casm_types::SerializedClass;
use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::VariantNames;

use crate::communication::CLASS_MANAGER_REQUEST_LABELS;

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
        LabeledMetricCounter {
            N_CLASSES,
            "class_manager_n_classes", "Number of classes, by label (regular, deprecated)",
            init = 0 ,
            labels = CAIRO_CLASS_TYPE_LABELS
        },
        LabeledMetricHistogram {
            CLASS_SIZES,
            "class_manager_class_sizes",
            "Size of the classes in bytes, labeled by type (sierra, casm, deprecated casm)",
            labels = CLASS_OBJECT_TYPE_LABELS
        },
    },
    Infra => {
        LabeledMetricHistogram {
            CLASS_MANAGER_LABELED_PROCESSING_TIMES_SECS,
            "class_manager_labeled_processing_times_secs",
            "Request processing times of the class manager, per label (secs)",
            labels = CLASS_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CLASS_MANAGER_LABELED_QUEUEING_TIMES_SECS,
            "class_manager_labeled_queueing_times_secs",
            "Request queueing times of the class manager, per label (secs)",
            labels = CLASS_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CLASS_MANAGER_LABELED_LOCAL_RESPONSE_TIMES_SECS,
            "class_manager_labeled_local_response_times_secs",
            "Request local response times of the class manager, per label (secs)",
            labels = CLASS_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CLASS_MANAGER_LABELED_REMOTE_RESPONSE_TIMES_SECS,
            "class_manager_labeled_remote_response_times_secs",
            "Request remote response times of the class manager, per label (secs)",
            labels = CLASS_MANAGER_REQUEST_LABELS
        },
        LabeledMetricHistogram {
            CLASS_MANAGER_LABELED_REMOTE_CLIENT_COMMUNICATION_FAILURE_TIMES_SECS,
            "class_manager_labeled_remote_client_communication_failure_times_secs",
            "Request communication failure times of the class manager, per label (secs)",
            labels = CLASS_MANAGER_REQUEST_LABELS
        },
    },
);

pub(crate) fn increment_n_classes(cls_type: CairoClassType) {
    N_CLASSES.increment(1, &[(CAIRO_CLASS_TYPE_LABEL, cls_type.into())]);
}

pub(crate) fn record_class_size<T>(class_type: ClassObjectType, class: &SerializedClass<T>) {
    let class_size = class.size().unwrap_or_else(|_| {
        panic!("Illegally formatted {} class, should not have gotten into the system.", class_type)
    });
    let class_size = u32::try_from(class_size).unwrap_or_else(|_| {
        panic!(
            "{} class size {} is bigger than what is allowed,
            should not have gotten into the system.",
            class_type, class_size
        )
    });

    CLASS_SIZES.record(class_size, &[(CLASS_OBJECT_TYPE_LABEL, class_type.into())]);
}

pub(crate) fn register_metrics() {
    N_CLASSES.register();
    CLASS_SIZES.register();
}
