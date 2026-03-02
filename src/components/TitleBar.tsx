import { getCurrentWindow } from "@tauri-apps/api/window";
import { Terminal, Minus, Square, X } from "lucide-react";

export function TitleBar() {
  return (
    <div
      className="flex-none h-8 bg-gray-900 flex items-center px-3 select-none drag-region"
      onDoubleClick={() => getCurrentWindow().toggleMaximize()}
    >
      <div className="flex items-center gap-2 pointer-events-none">
        <Terminal size={14} className="text-indigo-500" />
        <span className="text-[11px] text-gray-500 font-medium tracking-wide">
          shuttle-io
        </span>
      </div>
      <div className="flex-1" />
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
    </div>
  );
}
