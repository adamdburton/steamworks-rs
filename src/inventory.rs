use super::*;
use crate::sys;
use std::sync::Arc;
use std::time::Duration;

const CALLBACK_BASE_ID: i32 = 1300; // Adjust this base ID as needed for Inventory

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

    fn wait_for_result_and_get_items(
        &self,
        result_handle: sys::SteamInventoryResult_t,
    ) -> Result<Vec<SteamItemDetails>, InventoryError> {
        const MAX_ATTEMPTS: u32 = 100;
        const WAIT_DURATION: Duration = Duration::from_millis(100);

        for _ in 0..MAX_ATTEMPTS {
            unsafe {
                let result =
                    sys::SteamAPI_ISteamInventory_GetResultStatus(self.inventory, result_handle);
                if result == sys::EResult::k_EResultOK {
                    return self.get_result_items(result_handle);
                }
            }
            std::thread::sleep(WAIT_DURATION);
        }
        Err(InventoryError::Timeout)
    }

    fn get_result_items(
        &self,
        result_handle: sys::SteamInventoryResult_t,
    ) -> Result<Vec<SteamItemDetails>, InventoryError> {
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

            let mut items_array: Vec<sys::SteamItemDetails_t> =
                vec![std::mem::zeroed(); items_count as usize];
            if sys::SteamAPI_ISteamInventory_GetResultItems(
                self.inventory,
                result_handle,
                items_array.as_mut_ptr(),
                &mut items_count,
            ) {
                Ok(items_array
                    .into_iter()
                    .map(|details| SteamItemDetails {
                        item_id: SteamItemInstanceID(details.m_itemId),
                        definition: SteamItemDef(details.m_iDefinition),
                        quantity: details.m_unQuantity,
                        flags: details.m_unFlags,
                    })
                    .collect())
            } else {
                Err(InventoryError::GetResultItemsFailed)
            }
        }
    }

    fn destroy_result(&self, result_handle: sys::SteamInventoryResult_t) {
        unsafe {
            sys::SteamAPI_ISteamInventory_DestroyResult(self.inventory, result_handle);
        }
    }

    pub fn consume_item(
        &self,
        item_id: SteamItemInstanceID,
        quantity: u32,
    ) -> Result<(), InventoryError> {
        let result_handle = self.internal_consume_item(item_id, quantity)?;
        self.destroy_result(result_handle);
        Ok(())
    }

    fn internal_consume_item(
        &self,
        item_id: SteamItemInstanceID,
        quantity: u32,
    ) -> Result<sys::SteamInventoryResult_t, InventoryError> {
        let mut result_handle = sys::k_SteamInventoryResultInvalid;
        unsafe {
            if sys::SteamAPI_ISteamInventory_ConsumeItem(
                self.inventory,
                &mut result_handle,
                item_id.0,
                quantity,
            ) {
                Ok(result_handle)
            } else {
                Err(InventoryError::OperationFailed)
            }
        }
    }

    pub fn start_purchase<F>(&self, items: &[(SteamItemDef, u32)], cb: F)
    where
        F: FnOnce(Result<StartPurchaseResult, SteamError>) + 'static + Send,
    {
        if items.is_empty() {
            cb(Err(SteamError::InvalidParameter));
            return;
        }

        let (item_defs, quantities): (Vec<_>, Vec<_>) = items
            .iter()
            .map(|(def, quantity)| (def.0, *quantity))
            .unzip();

        unsafe {
            let api_call = sys::SteamAPI_ISteamInventory_StartPurchase(
                self.inventory,
                item_defs.as_ptr(),
                quantities.as_ptr(),
                items.len() as u32,
            );

            if api_call == sys::k_uAPICallInvalid {
                cb(Err(SteamError::InvalidParameter));
            } else {
                register_call_result::<sys::SteamInventoryStartPurchaseResult_t, _, _>(
                    &self._inner,
                    api_call,
                    CALLBACK_BASE_ID + 1, // Adjust this ID as needed
                    move |v, io_error| {
                        cb(if io_error {
                            Err(SteamError::IOFailure)
                        } else {
                            match v.m_result {
                                sys::EResult::k_EResultOK => Ok(StartPurchaseResult {
                                    order_id: v.m_ulOrderID,
                                    trans_id: v.m_ulTransID,
                                }),
                                _ => Err(SteamError::from(v.m_result)),
                            }
                        })
                    },
                );
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

#[derive(Clone, Debug)]
pub struct SteamItemPrice {
    pub item_def: SteamItemDef,
    pub price: u64,
    pub base_price: u64,
}

/// Represents a unique identifier for an inventory item instance.
#[derive(Clone, Debug)]
pub struct SteamItemInstanceID(pub u64);

/// Represents a unique identifier for an item definition.
#[derive(Clone, Debug)]
pub struct SteamItemDef(pub i32);

#[derive(Clone, Debug)]
pub struct StartPurchaseResult {
    pub order_id: u64,
    pub trans_id: u64,
}
