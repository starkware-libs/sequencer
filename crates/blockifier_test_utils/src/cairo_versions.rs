use starknet_api::transaction::TransactionVersion;
use strum_macros::EnumIter;

#[derive(Clone, Default, Hash, PartialEq, EnumIter, Eq, Copy, Debug)]
pub enum RunnableCairo1 {
    #[default]
    Casm,
    #[cfg(feature = "cairo_native")]
    Native,
}

impl RunnableCairo1 {
    pub fn is_cairo_native(&self) -> bool {
        match self {
            Self::Casm => false,

            #[cfg(feature = "cairo_native")]
            Self::Native => true,
        }
    }
}

// TODO(Aviv, 14/7/2024): Move from test utils module, and use it in ContractClassVersionMismatch
// error.
#[derive(Clone, Default, Hash, PartialEq, Eq, Copy, Debug)]
pub enum CairoVersion {
    #[default]
    Cairo0,
    Cairo1(RunnableCairo1),
}

impl CairoVersion {
    // A declare transaction of the given version, can be used to declare contracts of the returned
    // cairo version.
    // TODO(Dori): Make TransactionVersion an enum and use match here.
    pub fn from_declare_tx_version(tx_version: TransactionVersion) -> Self {
        if tx_version == TransactionVersion::ZERO || tx_version == TransactionVersion::ONE {
            CairoVersion::Cairo0
        } else if tx_version == TransactionVersion::TWO || tx_version == TransactionVersion::THREE {
            CairoVersion::Cairo1(RunnableCairo1::Casm)
        } else {
            panic!("Transaction version {tx_version:?} is not supported.")
        }
    }

    pub fn is_cairo0(&self) -> bool {
        match self {
            Self::Cairo0 => true,
            Self::Cairo1(_) => false,
        }
    }

    pub fn is_cairo_native(&self) -> bool {
        match self {
            Self::Cairo0 => false,

            Self::Cairo1(runnable_cairo1) => runnable_cairo1.is_cairo_native(),
        }
    }
}
