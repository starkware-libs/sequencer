use crate::presets::types::flavors::{FlavorFields, InterferenceFlavor};
use crate::presets::types::storage::{StorageLayout, StorageLayoutName};

pub mod flavors;
pub mod storage;

pub struct Preset {
    flavor_fields: FlavorFields,
    storage_layout: StorageLayout,
}

impl Preset {
    pub fn new(flavor_fields: FlavorFields, storage_layout: StorageLayout) -> Self {
        let preset = Self { flavor_fields, storage_layout };
        preset.validate();
        preset
    }

    fn validate(&self) {
        if self.flavor_fields.interference_fields.interference_type != InterferenceFlavor::None
            && !self.storage_layout.supports_interference()
        {
            panic!(
                "Storage layout {} does not support interference",
                self.storage_layout.short_name()
            );
        }
    }
}
