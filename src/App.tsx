import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface ExplorerState {
  path: string;
  selected_files: string[];
}

function App() {
  const [explorerState, setExplorerState] = useState<ExplorerState | null>(null);
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState(false);

  async function getExplorerStatus() {
    setLoading(true);
    try {
      const state = await invoke<ExplorerState>("get_explorer_status");
      console.log("Explorer State:", state);
      setExplorerState(state);
      setError("");
    } catch (e) {
      console.error("Error:", e);
      setError(String(e));
      setExplorerState(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="flex flex-col items-center justify-center p-8 bg-gray-50 min-h-screen text-gray-800">
      <h1 className="text-3xl font-bold mb-8 text-gray-900">Shuttle IO - Explorer integration</h1>

      <div className="w-full max-w-md space-y-4">
        <button
          onClick={getExplorerStatus}
          disabled={loading}
          className="w-full px-6 py-3 bg-indigo-600 text-white font-semibold rounded-lg shadow-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-opacity-75 disabled:opacity-50 transition"
        >
          {loading ? "Scanning..." : "Get Active Explorer Info"}
        </button>

        {error && (
          <div className="p-4 bg-red-100 border-l-4 border-red-500 text-red-700">
            <p className="font-bold">Error</p>
            <p>{error}</p>
          </div>
        )}

        {explorerState && (
          <div className="bg-white p-6 rounded-lg shadow-lg border border-gray-200">
            <div className="mb-4">
              <h3 className="text-sm font-uppercase tracking-wide text-gray-500 font-semibold">Current Path</h3>
              <code className="block mt-1 p-2 bg-gray-100 rounded text-sm break-all font-mono text-gray-800">
                {explorerState.path || "(No path found)"}
              </code>
            </div>

            <div>
              <h3 className="text-sm font-uppercase tracking-wide text-gray-500 font-semibold mb-2">
                Selected Files ({explorerState.selected_files.length})
              </h3>
              {explorerState.selected_files.length > 0 ? (
                <ul className="space-y-1 max-h-60 overflow-y-auto">
                  {explorerState.selected_files.map((file, idx) => (
                    <li key={idx} className="text-sm p-2 bg-gray-50 rounded border border-gray-100 truncate">
                      {file}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm text-gray-400 italic">No files selected</p>
              )}
            </div>
          </div>
        )}
      </div>
    </main>
  );
}

export default App;
