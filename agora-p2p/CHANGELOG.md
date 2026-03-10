# Changelog

## v0.2.0

### Breaking Changes

- **broadcast_room_message renamed to broadcast_grove_message**
- **room_id field renamed to grove_id in AmpMessage**
- **FuelOffer/FuelClaim/FuelReceipt renamed to MettleOffer/MettleClaim/MettleReceipt**
- **fuel capability renamed to mettle in Capabilities**
- **agora-app removed from workspace**

### Internal Improvements

- **S-02**: RwLock<u64> sequence counters replaced with AtomicU64
- Improved error handling for peer removal events
