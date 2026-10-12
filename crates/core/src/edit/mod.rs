//! Edit operations. Each `plan_*` function computes a complete candidate document (and, for
//! catalogue edits, catalogue) without touching its input; a `Rejection` means nothing changes.

mod cable;
mod catalog;
mod device;
mod document;
mod port;
mod rack;

pub use cable::*;
pub use catalog::*;
pub use device::*;
pub use document::*;
pub use port::*;
pub use rack::*;

use crate::ids::{CableId, DeviceId, PortId, RackId};
use crate::model::{Catalog, Document};
use crate::placement::PlacementError;
use crate::port_grid::PortPlacementError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectId {
    Rack(RackId),
    Device(DeviceId),
    Port(PortId),
    Cable(CableId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub document: Document,
    /// The object created or moved by this plan, if any.
    pub subject: Option<ObjectId>,
}

/// A catalogue edit: the new catalogue, and the document with its links updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPlan {
    pub document: Document,
    pub catalog: Catalog,
    /// The object created or moved by this plan, if any.
    pub subject: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("the object no longer exists")]
    NotFound,
    #[error("the name '{0}' is already used here")]
    NameTaken(String),
    #[error("the height must be at least 1U")]
    ZeroHeight,
    #[error("the device is taller than the rack")]
    TooTall,
    #[error("there is not enough free space above")]
    NoRoomAbove,
    #[error("there is not enough free space below")]
    NoRoomBelow,
    #[error("equipment would end up outside the rack")]
    EquipmentOutside,
    #[error("ports would end up outside the device face")]
    PortsOutside,
    #[error("the position is outside the device face")]
    OutOfGrid,
    #[error("there are not enough free cells to make room")]
    NoRoomForPorts,
    #[error("a port can only be placed on its own device")]
    WrongDevice,
    #[error("the height must be at most {max}U")]
    HeightAboveLimit { max: u32 },
    #[error("a cable needs two different ports")]
    SamePort,
    #[error("port '{0}' already has a cable")]
    PortInUse(String),
    #[error("the model '{0}' is already in the catalogue")]
    ModelTaken(String),
    #[error("the model must not be empty")]
    ModelRequired,
    #[error("the device was not placed from a catalogue model")]
    NotLinked,
    #[error("the model is not in the catalogue")]
    ModelMissing,
}

impl From<PlacementError> for Rejection {
    fn from(e: PlacementError) -> Self {
        match e {
            PlacementError::TooTall => Rejection::TooTall,
            PlacementError::NoRoomAbove => Rejection::NoRoomAbove,
            PlacementError::NoRoomBelow => Rejection::NoRoomBelow,
        }
    }
}

impl From<PortPlacementError> for Rejection {
    fn from(e: PortPlacementError) -> Self {
        match e {
            PortPlacementError::OutOfGrid => Rejection::OutOfGrid,
            PortPlacementError::NoRoom => Rejection::NoRoomForPorts,
        }
    }
}
