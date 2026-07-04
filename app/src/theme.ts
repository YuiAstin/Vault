// --- Theme System ---

export interface ThemeColors {
  bg: string;
  bgCard: string;
  bgInput: string;
  accent: string;
  accentHover: string;
  text: string;
  textDim: string;
  border: string;
  success: string;
  danger: string;
}

export interface ThemeConfig {
  id: string;
  name: string;
  colors: ThemeColors;
}

// --- Preset Themes ---
export const PRESETS: ThemeConfig[] = [
  {
    id: "dark",
    name: "Dark (Default)",
    colors: {
      bg: "#1a1a2e",
      bgCard: "#16213e",
      bgInput: "#0f3460",
      accent: "#e94560",
      accentHover: "#ff6b6b",
      text: "#eaeaea",
      textDim: "#a0a0b0",
      border: "#2a2a4a",
      success: "#4ecdc4",
      danger: "#e94560",
    },
  },
  {
    id: "light",
    name: "Light",
    colors: {
      bg: "#f5f5f7",
      bgCard: "#ffffff",
      bgInput: "#e8e8ec",
      accent: "#6366f1",
      accentHover: "#818cf8",
      text: "#1a1a2e",
      textDim: "#6b7280",
      border: "#d1d5db",
      success: "#10b981",
      danger: "#ef4444",
    },
  },
  {
    id: "midnight",
    name: "Midnight",
    colors: {
      bg: "#0d1117",
      bgCard: "#161b22",
      bgInput: "#21262d",
      accent: "#58a6ff",
      accentHover: "#79c0ff",
      text: "#e6edf3",
      textDim: "#8b949e",
      border: "#30363d",
      success: "#3fb950",
      danger: "#f85149",
    },
  },
  {
    id: "ocean",
    name: "Ocean",
    colors: {
      bg: "#0f172a",
      bgCard: "#1e293b",
      bgInput: "#334155",
      accent: "#06b6d4",
      accentHover: "#22d3ee",
      text: "#f1f5f9",
      textDim: "#94a3b8",
      border: "#475569",
      success: "#34d399",
      danger: "#fb7185",
    },
  },
  {
    id: "forest",
    name: "Forest",
    colors: {
      bg: "#1a2e1a",
      bgCard: "#1e3a1e",
      bgInput: "#2d4a2d",
      accent: "#4ade80",
      accentHover: "#86efac",
      text: "#ecfdf5",
      textDim: "#9ca3af",
      border: "#365836",
      success: "#34d399",
      danger: "#f87171",
    },
  },
  {
    id: "rose",
    name: "Rose",
    colors: {
      bg: "#1c1017",
      bgCard: "#2a1520",
      bgInput: "#3d1f2e",
      accent: "#f472b6",
      accentHover: "#f9a8d4",
      text: "#fdf2f8",
      textDim: "#a78bab",
      border: "#4a2639",
      success: "#4ade80",
      danger: "#fb7185",
    },
  },
  {
    id: "amber",
    name: "Amber",
    colors: {
      bg: "#1c1a0e",
      bgCard: "#2a2614",
      bgInput: "#3d371c",
      accent: "#f59e0b",
      accentHover: "#fbbf24",
      text: "#fefce8",
      textDim: "#a8a29e",
      border: "#4a4320",
      success: "#4ecdc4",
      danger: "#ef4444",
    },
  },
  {
    id: "nord",
    name: "Nord",
    colors: {
      bg: "#2e3440",
      bgCard: "#3b4252",
      bgInput: "#434c5e",
      accent: "#88c0d0",
      accentHover: "#8fbcbb",
      text: "#eceff4",
      textDim: "#d8dee9",
      border: "#4c566a",
      success: "#a3be8c",
      danger: "#bf616a",
    },
  },
];

const STORAGE_KEY = "vault-theme";

// --- Persistence ---
export function loadSavedTheme(): ThemeConfig {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as ThemeConfig;
      if (parsed.id && parsed.name && parsed.colors && parsed.colors.bg) {
        return parsed;
      }
    }
  } catch {
    // Ignore parse errors, use default
  }
  return PRESETS[0];
}

export function saveTheme(theme: ThemeConfig): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(theme));
}

// --- Apply Theme to DOM ---
export function applyTheme(theme: ThemeConfig): void {
  const root = document.documentElement;
  root.style.setProperty("--bg", theme.colors.bg);
  root.style.setProperty("--bg-card", theme.colors.bgCard);
  root.style.setProperty("--bg-input", theme.colors.bgInput);
  root.style.setProperty("--accent", theme.colors.accent);
  root.style.setProperty("--accent-hover", theme.colors.accentHover);
  root.style.setProperty("--text", theme.colors.text);
  root.style.setProperty("--text-dim", theme.colors.textDim);
  root.style.setProperty("--border", theme.colors.border);
  root.style.setProperty("--success", theme.colors.success);
  root.style.setProperty("--danger", theme.colors.danger);
}

// --- Create Custom Theme from overrides ---
export function createCustomTheme(base: ThemeConfig, overrides: Partial<ThemeColors>): ThemeConfig {
  return {
    id: "custom",
    name: "Custom",
    colors: { ...base.colors, ...overrides },
  };
}
