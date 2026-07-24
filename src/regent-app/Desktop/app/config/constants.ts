// App-wide constants (never inline). Package parity is CI-checked against the
// Tauri/Rust versions, so the UI reads the package instead of copying it again.
import pkg from '../../package.json';

export const APP_VERSION = pkg.version;
