use super::*;
use std::sync::Arc;
use std::time::Duration;
use crate::sys;

pub struct Inventory<Manager> {
    pub(crate) inventory: *mut sys::ISteamInventory,
    pub(crate) _inner: Arc<Inner<Manager>>,
}

impl<Manager> Inventory<Manager> {
    /// Retrieves all items in the user's Steam inventory.
    pub fn get_all_items(&self) -> Result<Vec<SteamItemDetails>, InventoryError> {
        let result_handle = self.request_all_items()?;
        let items = self.wait_for_result_and_get_items(result_handle)?;
        self.destroy_result(result_handle);
        Ok(items)
    }

    fn request_all_items(&self) -> Result<sys::SteamInventoryResult_t, InventoryError> {
        let mut result_handle = sys::k_SteamInventoryResultInvalid;
        unsafe {
            if sys::SteamAPI_ISteamInventory_GetAllItems(self.inventory, &mut result_handle) {
                Ok(result_handle)
            } else {
                Err(InventoryError::OperationFailed)
            }
        }
    }

    fn wait_for_result_and_get_items(&self, result_handle: sys::SteamInventoryResult_t) -> Result<Vec<SteamItemDetails>, InventoryError> {
        const MAX_ATTEMPTS: u32 = 100;
        const WAIT_DURATION: Duration = Duration::from_millis(100);

        for _ in 0..MAX_ATTEMPTS {
            unsafe {
                let result = sys::SteamAPI_ISteamInventory_GetResultStatus(self.inventory, result_handle);
                if result == sys::EResult::k_EResultOK {
                    return self.get_result_items(result_handle);
                }
            }
            std::thread::sleep(WAIT_DURATION);
        }
        Err(InventoryError::Timeout)
    }

    fn get_result_items(&self, result_handle: sys::SteamInventoryResult_t) -> Result<Vec<SteamItemDetails>, InventoryError> {
        unsafe {
            let mut items_count = 0;
            if !sys::SteamAPI_ISteamInventory_GetResultItems(
                self.inventory,
                result_handle,
                std::ptr::null_mut(),
                &mut items_count,
            ) {
                return Err(InventoryError::GetResultItemsFailed);
            }

            let mut items_array: Vec<sys::SteamItemDetails_t> = vec![std::mem::zeroed(); items_count as usize];
            if sys::SteamAPI_ISteamInventory_GetResultItems(
                self.inventory,
                result_handle,
                items_array.as_mut_ptr(),
                &mut items_count,
            ) {
                Ok(items_array.into_iter().map(|details| SteamItemDetails {
                    item_id: SteamItemInstanceID(details.m_itemId),
                    definition: SteamItemDef(details.m_iDefinition),
                    quantity: details.m_unQuantity,
                    flags: details.m_unFlags,
                }).collect())
            } else {
                Err(InventoryError::GetResultItemsFailed)
            }
        }
    }

    fn destroy_result(&self, result_handle: sys::SteamInventoryResult_t) {
        unsafe {
            sys::SteamAPI_ISteamInventory_DestroyResult(
                self.inventory,
                result_handle,
            );
        }
    }
}

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("The inventory operation failed")]
    OperationFailed,
    #[error("Failed to retrieve result items")]
    GetResultItemsFailed,
    #[error("Invalid input")]
    InvalidInput,
    #[error("Timeout waiting for inventory result")]
    Timeout,
}

/// Represents an individual inventory item with its unique details.
#[derive(Clone, Debug)]
pub struct SteamItemDetails {
    pub item_id: SteamItemInstanceID,
    pub definition: SteamItemDef,
    pub quantity: u16,
    pub flags: u16,
}

/// Represents a unique identifier for an inventory item instance.
#[derive(Clone, Debug)]
pub struct SteamItemInstanceID(pub u64);

/// Represents a unique identifier for an item definition.
#[derive(Clone, Debug)]
pub struct SteamItemDef(pub i32);