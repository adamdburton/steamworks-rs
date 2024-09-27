use super::*;
use std::sync::Arc;
use crate::sys;

pub struct Inventory<Manager> {
    pub(crate) inventory: *mut sys::ISteamInventory,
    pub(crate) _inner: Arc<Inner<Manager>>,
}

impl<Manager> Inventory<Manager> {
    /// Retrieves all items in the user's Steam inventory.
    pub fn get_all_items(&self) -> Result<Vec<SteamItemDetails>, InventoryError> {
        let result_handle = self.internal_get_all_items()?;
        let items = self.internal_get_result_items(result_handle)?;
        self.internal_destroy_result(result_handle);
        Ok(items)
    }

    fn internal_get_all_items(&self) -> Result<sys::SteamInventoryResult_t, InventoryError> {
        let mut result_handle = sys::k_SteamInventoryResultInvalid;
        unsafe {
            if sys::SteamAPI_ISteamInventory_GetAllItems(self.inventory, &mut result_handle) {
                Ok(result_handle)
            } else {
                Err(InventoryError::OperationFailed)
            }
        }
    }

    fn internal_get_result_items(&self, result_handle: sys::SteamInventoryResult_t) -> Result<Vec<SteamItemDetails>, InventoryError> {
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

    fn internal_destroy_result(&self, result_handle: sys::SteamInventoryResult_t) {
        unsafe {
            sys::SteamAPI_ISteamInventory_DestroyResult(
                self.inventory,
                result_handle,
            );
        }
    }

    pub fn consume_item(&self, item_id: SteamItemInstanceID, quantity: u32) -> Result<(), InventoryError> {
        let result_handle = self.internal_consume_item(item_id, quantity)?;
        self.internal_destroy_result(result_handle);
        Ok(())
    }

    fn internal_consume_item(&self, item_id: SteamItemInstanceID, quantity: u32) -> Result<sys::SteamInventoryResult_t, InventoryError> {
        let mut result_handle = sys::k_SteamInventoryResultInvalid;
        unsafe {
            if sys::SteamAPI_ISteamInventory_ConsumeItem(self.inventory, &mut result_handle, item_id.0, quantity) {
                Ok(result_handle)
            } else {
                Err(InventoryError::OperationFailed)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SteamItemDetails {
    pub item_id: SteamItemInstanceID,
    pub definition: SteamItemDef,
    pub quantity: u16,
    pub flags: u16,
}

#[derive(Clone, Debug)]
pub struct SteamItemInstanceID(pub u64);

#[derive(Clone, Debug)]
pub struct SteamItemDef(pub i32);

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("The inventory operation failed")]
    OperationFailed,
    #[error("Failed to retrieve result items")]
    GetResultItemsFailed,
    #[error("Invalid input")]
    InvalidInput,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_items() {
        let client = Client::init().unwrap();
        
        match client.inventory().get_all_items() {
            Ok(items) => {
                assert!(!items.is_empty(), "No items received");
                println!("Result items: {:?}", items);
            },
            Err(e) => panic!("Failed to get inventory items: {:?}", e),
        }
    }
}