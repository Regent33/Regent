//! Everyday tools — the on-the-go utilities (weather, conversion, QR codes,
//! timezones, reminders, …). Keyless by design: network-backed ones use free
//! public APIs (Open-Meteo, frankfurter.app, dictionaryapi.dev); the rest are
//! fully offline. All register under the `everyday` toolset and ship deferred
//! (`load_tools`), so they cost zero prompt tokens until actually needed.

mod calc;
mod convert;
mod date_calc;
mod dictionary;
mod geo;
mod http;
mod qr_code;
mod random_gen;
mod reminder;
mod reminder_time;
mod sun_moon;
mod weather;
mod world_time;

use crate::application::catalog::ToolCatalog;
use regent_kernel::RegentError;
use std::sync::Arc;

/// Registers the whole everyday toolset. Registration is deliberate and
/// collision-checked like every core tool.
pub fn register_everyday_tools(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(calc::definition(), Arc::new(calc::CalcTool))?;
    catalog.register(convert::definition(), Arc::new(convert::ConvertTool))?;
    catalog.register(date_calc::definition(), Arc::new(date_calc::DateCalcTool))?;
    catalog.register(
        dictionary::definition(),
        Arc::new(dictionary::DictionaryTool),
    )?;
    catalog.register(qr_code::definition(), Arc::new(qr_code::QrCodeTool))?;
    catalog.register(
        random_gen::definition(),
        Arc::new(random_gen::RandomGenTool),
    )?;
    catalog.register(reminder::definition(), Arc::new(reminder::ReminderTool))?;
    catalog.register(sun_moon::definition(), Arc::new(sun_moon::SunMoonTool))?;
    catalog.register(weather::definition(), Arc::new(weather::WeatherTool))?;
    catalog.register(
        world_time::definition(),
        Arc::new(world_time::WorldTimeTool),
    )?;
    Ok(())
}
