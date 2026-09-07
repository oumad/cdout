import { useState, useEffect } from "react";
import type { PlatformInfo } from "../types";
import * as api from "../utils/tauri";

/**
 * Host-OS description, fetched once from the backend.
 *
 * The backend is the authority — it builds the system prompt, so anything the
 * UI says about the shell has to come from the same place or the two can
 * disagree. The fallback below only covers the first render or a failed
 * invoke: labels degrade to the wrong file-manager name for a frame, and the
 * synthesized tool name is one the registry accepts on either platform.
 */
const FALLBACK: PlatformInfo = navigator.userAgent.includes("Mac")
  ? {
      os: "macos",
      os_name: "macOS",
      file_manager: "Finder",
      shell_name: "zsh",
      shell_tool_name: "run_shell",
      read_list_hint: "",
    }
  : {
      os: "windows",
      os_name: "Windows",
      file_manager: "Explorer",
      shell_name: "PowerShell",
      shell_tool_name: "run_powershell",
      read_list_hint: "",
    };

export function usePlatform(): PlatformInfo {
  const [platform, setPlatform] = useState<PlatformInfo>(FALLBACK);

  useEffect(() => {
    let cancelled = false;
    api
      .getPlatformInfo()
      .then((info) => {
        if (!cancelled) setPlatform(info);
      })
      .catch(() => {
        /* keep the fallback — every field has a usable default */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return platform;
}
