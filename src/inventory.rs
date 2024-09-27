use super::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::sys;

pub struct Inventory<Manager> {
    inventory: *mut sys::ISteamInventory,
    _inner: Arc<Inner<Manager>>,
    pending_results: Mutex<Vec<sys::SteamInventoryResult_t>>,
}

impl<Manager> Inventory<Manager> {
    pub(crate) fn new(inventory: *mut sys::ISteamInventory, inner: Arc<Inner<Manager>>) -> Self {
        Self {
            inventory,
            _inner: inner,
            pending_results: Mutex::new(Vec::new()),
        }
    }

    pub fn get_all_items(&self) -> Result<Vec<SteamItemDetails>, InventoryError> {
        let result_handle = self.request_all_items()?;
        let items = self.wait_for_result_and_get_items(result_handle)?;
        Ok(items)
    }

    fn request_all_items(&self) -> Result<sys::SteamInventoryResult_t, InventoryError> {
        let mut result_handle = sys::k_SteamInventoryResultInvalid;
        unsafe {
            if sys::SteamAPI_ISteamInventory_GetAllItems(self.inventory, &mut result_handle) {
                self.pending_results.lock().unwrap().push(result_handle);
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
                if sys::SteamAPI_ISteamInventory_GetResultStatus(self.inventory, result_handle) == sys::EResult::k_EResultOK {
                    return self.get_result_items(result_handle);
                }
            }
            std::thread::sleep(WAIT_DURATION);
        }
        Err(InventoryError::Timeout)
    }

    fn get_result_items(&self, result_handle: sys::SteamInventoryResult_t) -> Result<Vec<SteamItemDetails>, InventoryError> {
        let mut items_count = 0;
        unsafe {
            if !sys::SteamAPI_ISteamInventory_GetResultItems(
                self.inventory,
                result_handle,
                std::ptr::null_mut(),
                &mut items_count,
            ) {
                return Err(InventoryError::GetResultItemsFailed);
            }

            let mut items_array: Vec<sys::SteamItemDetails_t> = Vec::with_capacity(items_count as usize);
            if sys::SteamAPI_ISteamInventory_GetResultItems(
                self.inventory,
                result_handle,
                items_array.as_mut_ptr(),
                &mut items_count,
            ) {
                items_array.set_len(items_count as usize);
                let items = items_array.into_iter().map(|details| SteamItemDetails {
                    item_id: SteamItemInstanceID(details.m_itemId),
                    definition: SteamItemDef(details.m_iDefinition),
                    quantity: details.m_unQuantity,
                    flags: details.m_unFlags,
                }).collect();
                Ok(items)
            } else {
                Err(InventoryError::GetResultItemsFailed)
            }
        }
    }
}

impl<Manager> Drop for Inventory<Manager> {
    fn drop(&mut self) {
        let pending_results = std::mem::take(&mut *self.pending_results.lock().unwrap());
        for result_handle in pending_results {
            unsafe {
                sys::SteamAPI_ISteamInventory_DestroyResult(self.inventory, result_handle);
            }
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