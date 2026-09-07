import { getCurrentWindow } from "@tauri-apps/api/window";
import { Terminal, Minus, Square, X } from "lucide-react";
import { usePlatform } from "../hooks";

/**
 * The window is undecorated on both platforms, so the controls are ours to
 * draw — which means their placement is ours to get right. Windows puts
 * minimize/maximize/close at the top right; macOS puts close/minimize/zoom at
 * the top left as coloured dots. Shipping the Windows layout on a Mac is the
 * kind of detail that makes an app feel ported rather than native.
 *
 * "Close" hides rather than quits on both: the app lives in the tray/menu bar.
 */
function WindowsControls() {
  return (
    <div className="flex items-center no-drag -mr-2">
      <button
        onClick={() => getCurrentWindow().minimize()}
        className="w-10 h-8 flex items-center justify-center hover:bg-gray-800 text-gray-500 hover:text-gray-300 transition"
        title="Minimize"
      >
        <Minus size={14} />
      </button>
      <button
        onClick={() => getCurrentWindow().toggleMaximize()}
        className="w-10 h-8 flex items-center justify-center hover:bg-gray-800 text-gray-500 hover:text-gray-300 transition"
        title="Maximize"
      >
        <Square size={10} />
      </button>
      <button
        onClick={() => getCurrentWindow().hide()}
        className="w-10 h-8 flex items-center justify-center hover:bg-red-600 text-gray-500 hover:text-white transition"
        title="Close"
      >
        <X size={14} />
      </button>
    </div>
  );
}

function MacControls() {
  // System colours, so the dots read as window controls rather than as
  // decoration. Order is fixed by the platform: close, minimize, zoom.
  const dot =
    "w-3 h-3 rounded-full transition-opacity opacity-90 hover:opacity-100";
  return (
    <div className="flex items-center gap-2 no-drag mr-3 group">
      <button
        onClick={() => getCurrentWindow().hide()}
        className={`${dot} bg-[#ff5f57]`}
        title="Close"
        aria-label="Close"
      />
      <button
        onClick={() => getCurrentWindow().minimize()}
        className={`${dot} bg-[#febc2e]`}
        title="Minimize"
        aria-label="Minimize"
      />
      <button
        onClick={() => getCurrentWindow().toggleMaximize()}
        className={`${dot} bg-[#28c840]`}
        title="Zoom"
        aria-label="Zoom"
      />
    </div>
  );
}

export function TitleBar() {
  const platform = usePlatform();
  const isMac = platform.os === "macos";

  return (
    <div
      className="flex-none h-8 bg-gray-900 flex items-center px-3 select-none drag-region"
      onDoubleClick={() => getCurrentWindow().toggleMaximize()}
    >
      {isMac && <MacControls />}
      <div className="flex items-center gap-2 pointer-events-none">
        <Terminal size={14} className="text-indigo-500" />
        <span className="text-[11px] text-gray-500 font-medium tracking-wide">
          cdout
        </span>
      </div>
      <div className="flex-1" />
      {!isMac && <WindowsControls />}
    </div>
  );
}
