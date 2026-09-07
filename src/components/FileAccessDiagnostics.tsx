import { useState, useCallback, useEffect } from "react";
import {
  CheckCircle,
  XCircle,
  RefreshCw,
  ShieldAlert,
  ExternalLink,
  Loader2,
} from "lucide-react";
import type { ExplorerDebugInfo, PlatformInfo } from "../types";
import * as api from "../utils/tauri";

/**
 * Explains why file context is empty.
 *
 * On macOS, reading the Finder selection requires an Apple-events grant. The
 * consent dialog appears exactly once — deny it (or miss it) and cdout looks
 * permanently broken: the toolbar shows nothing selected, with no hint that a
 * checkbox is responsible. The backend already distinguishes that case
 * (`automation_authorized`), so this surfaces it and deep-links the fix.
 *
 * On Windows there is no grant to give, so this reports which Explorer window
 * matched instead — the equivalent question there is "why did it pick that
 * tab", not "am I allowed".
 */
export function FileAccessDiagnostics({
  platform,
  onError,
}: {
  platform: PlatformInfo;
  onError?: (msg: string) => void;
}) {
  const [info, setInfo] = useState<ExplorerDebugInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setInfo(await api.getExplorerDebug());
      setError(null);
    } catch (e) {
      // A hard failure here is itself the diagnosis, so it is rendered
      // rather than thrown away.
      setInfo(null);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function openSettings() {
    try {
      await api.openFileAccessSettings();
    } catch (e) {
      onError?.(String(e));
    }
  }

  const isMac = platform.os === "macos";
  // Treat an outright command failure as "not authorized" on macOS: the most
  // common cause is the TCC denial that the backend reports as an error.
  const denied = isMac && (info?.automation_authorized === false || !!error);
  const working = !denied && !error;

  return (
    <div className="space-y-4">
      <div
        className={`px-4 py-3 rounded-md border ${
          denied
            ? "bg-red-900/20 border-red-500/30"
            : working
              ? "bg-emerald-900/20 border-emerald-500/30"
              : "bg-gray-800/40 border-gray-700/50"
        }`}
      >
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-2.5 min-w-0">
            {loading ? (
              <Loader2 size={15} className="text-gray-500 animate-spin mt-0.5 shrink-0" />
            ) : denied ? (
              <ShieldAlert size={15} className="text-red-400 mt-0.5 shrink-0" />
            ) : working ? (
              <CheckCircle size={15} className="text-emerald-500 mt-0.5 shrink-0" />
            ) : (
              <XCircle size={15} className="text-gray-500 mt-0.5 shrink-0" />
            )}
            <div className="min-w-0">
              <div
                className={`text-sm font-medium ${
                  denied
                    ? "text-red-300"
                    : working
                      ? "text-emerald-300"
                      : "text-gray-300"
                }`}
              >
                {loading
                  ? `Checking ${platform.file_manager} access…`
                  : denied
                    ? `cdout cannot read ${platform.file_manager}`
                    : `${platform.file_manager} access is working`}
              </div>
              {!loading && (
                <div className="text-xs text-gray-400 mt-1 leading-relaxed">
                  {denied ? (
                    <>
                      Automation permission was denied, so the agent never sees
                      your open folder or selected files. Enable{" "}
                      <strong>cdout → {platform.file_manager}</strong> under
                      Privacy &amp; Security → Automation, then re-check.
                    </>
                  ) : info?.resolved_path ? (
                    <>
                      Reading <code className="text-gray-300">{info.resolved_path}</code>
                      {typeof info.selected_count === "number" && (
                        <> · {info.selected_count} file(s) selected</>
                      )}
                    </>
                  ) : (
                    <>
                      No {platform.file_manager} window is open right now, so
                      there is nothing to read. Open one and re-check.
                    </>
                  )}
                </div>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {denied && (
              <button
                onClick={openSettings}
                className="inline-flex items-center gap-1.5 px-2.5 py-1.5 bg-red-600/90 hover:bg-red-500 text-white rounded-md text-xs font-medium transition"
              >
                Open Settings <ExternalLink size={11} />
              </button>
            )}
            <button
              onClick={refresh}
              disabled={loading}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 bg-gray-800 hover:bg-gray-700 border border-gray-700 text-gray-200 rounded-md text-xs font-medium transition disabled:opacity-50"
            >
              <RefreshCw size={11} className={loading ? "animate-spin" : ""} />
              Re-check
            </button>
          </div>
        </div>
      </div>

      {(info || error) && (
        <details className="group">
          <summary className="text-xs text-gray-500 hover:text-gray-300 cursor-pointer select-none">
            Raw diagnostics
          </summary>
          <pre className="mt-2 text-[11px] text-gray-400 bg-black/40 border border-gray-800 rounded-md p-3 overflow-x-auto whitespace-pre-wrap break-all">
            {error ?? JSON.stringify(info, null, 2)}
          </pre>
        </details>
      )}
    </div>
  );
}
