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

    /// Initiates a request to retrieve prices for all purchasable items.
    ///
    /// The provided callback is invoked once the price request is complete.
    pub fn request_prices<F>(&self, cb: F)
    where
        F: FnOnce(Result<RequestPricesResult, InventoryError>) + 'static + Send,
    {
        unsafe {
            let api_call = sys::SteamAPI_ISteamInventory_RequestPrices(self.inventory);

            // Register the callback for SteamInventoryRequestPricesResult_t
            register_call_result::<sys::SteamInventoryRequestPricesResult_t, _, _>(
                &self._inner,
                api_call,
                CALLBACK_BASE_ID + 2, // Unique callback ID for request_prices
                move |v, io_error| {
                    if io_error {
                        cb(Err(InventoryError::OperationFailed));
                    } else {
                        match v.m_result {
                            sys::EResult::k_EResultOK => {
                                let string = CStr::from_ptr(v.m_rgchCurrency.as_ptr())
                                    .to_string_lossy()
                                    .into_owned();

                                cb(Ok(RequestPricesResult {
                                    result: v.m_result as i32,
                                    currency: string,
                                }));
                            }
                            _ => cb(Err(InventoryError::InvalidSteamResult)),
                        }
                    }
                },
            );
        }
    }

    /// Retrieves the number of items with valid pricing information.
    ///
    /// Must be called after a successful `request_prices`.
    ///
    /// # Returns
    ///
    /// * `Result<u32, InventoryError>` - The number of items with prices or an error.
    pub fn get_num_items_with_prices(&self) -> Result<u32, InventoryError> {
        unsafe {
            let num = sys::SteamAPI_ISteamInventory_GetNumItemsWithPrices(self.inventory);
            if num > 0 {
                Ok(num)
            } else {
                Err(InventoryError::NoPricesAvailable)
            }
        }
    }

    /// Retrieves the prices for items asynchronously.
    ///
    /// This method allocates the necessary arrays internally and passes the results
    /// to the provided callback.
    ///
    /// # Arguments
    ///
    /// * `cb` - A closure that takes a `Result<PricesDetailsArray, InventoryError>`.
    pub fn get_items_with_prices<F>(&self, cb: F)
    where
        F: FnOnce(Result<PricesDetailsArray, InventoryError>) + 'static + Send,
    {
        // First, retrieve the number of items with prices
        let num_items = match self.get_num_items_with_prices() {
            Ok(n) => n,
            Err(e) => {
                cb(Err(e));
                return;
            }
        };

        // If there are no items, return an empty array
        if num_items == 0 {
            cb(Ok(Vec::new()));
            return;
        }

        // Allocate the necessary arrays
        let mut item_defs: Vec<sys::SteamItemDef_t> = vec![0; num_items as usize];
        let mut current_prices: Vec<u64> = vec![0; num_items as usize];
        let mut base_prices: Vec<u64> = vec![0; num_items as usize];

        // Call the Steamworks API to get the prices
        let success = unsafe {
            sys::SteamAPI_ISteamInventory_GetItemsWithPrices(
                self.inventory,
                item_defs.as_mut_ptr(),
                current_prices.as_mut_ptr(),
                base_prices.as_mut_ptr(),
                num_items,
            )
        };

        if !success {
            cb(Err(InventoryError::GetPricesFailed));
            return;
        }

        // Construct the PricesDetailsArray
        let mut prices = Vec::with_capacity(num_items as usize);
        for i in 0..num_items as usize {
            prices.push(PriceDetails {
                item_def: SteamItemDef(item_defs[i]),
                current_price: current_prices[i],
                base_price: base_prices[i],
            });
        }

        cb(Ok(prices));
    }
}

// Called when a microtransaction authorization response is received
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SteamInventoryRequestPricesResult {
    pub m_result: i32,
    pub m_rgch_currency: String,
}

unsafe impl Callback for SteamInventoryRequestPricesResult {
    const ID: i32 = 152;
    const SIZE: i32 = std::mem::size_of::<sys::SteamInventoryRequestPricesResult_t>() as i32;

    unsafe fn from_raw(raw: *mut c_void) -> Self {
        let val = &mut *(raw as *mut sys::SteamInventoryRequestPricesResult_t);
        SteamInventoryRequestPricesResult {
            m_result: val.m_result as i32,
            m_rgch_currency: CStr::from_ptr(val.m_rgchCurrency.as_ptr())
                .to_string_lossy()
                .into_owned(),
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
    #[error("Invalid Steam API call result")]
    InvalidSteamResult,
    #[error("No prices available")]
    NoPricesAvailable,
    #[error("Failed to retrieve prices")]
    GetPricesFailed,
}

/// Represents the result of retrieving item prices.
pub type PricesDetailsArray = Vec<PriceDetails>;

/// Represents pricing details for an individual item.
#[derive(Clone, Debug)]
pub struct PriceDetails {
    pub item_def: SteamItemDef,
    pub current_price: u64,
    pub base_price: u64,
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

#[derive(Clone, Debug)]
pub struct RequestPricesResult {
    pub result: i32,
    pub currency: String,
}
