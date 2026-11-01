/** The window of the app's WebView: Tauri's bridge to the Rust side, which runs its commands. */
export type TauriWindow = Window &
  typeof globalThis & {
    __TAURI_INTERNALS__: { invoke(command: string, args?: unknown): Promise<unknown> };
  };
