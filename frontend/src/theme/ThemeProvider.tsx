import React, { useEffect } from "react";
import { StoreTheme } from "../types";

interface ThemeProviderProps {
  theme: StoreTheme;
  children: React.ReactNode;
}

export const ThemeProvider: React.FC<ThemeProviderProps> = ({
  theme,
  children,
}) => {
  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--pos-primary", theme.primary_color);
    root.style.setProperty("--pos-secondary", theme.secondary_color);
    root.style.setProperty("--pos-accent", theme.accent_color);
    root.style.setProperty("--pos-bg", theme.background_color);
    root.style.setProperty("--pos-surface", theme.surface_color);
    root.style.setProperty("--pos-text", theme.text_color);
    root.style.setProperty("--pos-muted", theme.muted_text_color);
    root.style.setProperty("--pos-font", theme.font_family);

    const radiusMap: Record<StoreTheme["border_radius"], string> = {
      sharp: "4px",
      rounded: "12px",
      pill: "22px",
    };
    root.style.setProperty(
      "--pos-radius",
      radiusMap[theme.border_radius] || "12px"
    );

    const densityMap: Record<StoreTheme["ui_density"], string> = {
      compact: "0.65rem",
      comfortable: "1rem",
      spacious: "1.35rem",
    };
    root.style.setProperty(
      "--pos-card-pad",
      densityMap[theme.ui_density] || "1rem"
    );

    document.title = `${theme.store_name} • POS System`;
  }, [theme]);

  return (
    <div
      className="pos-theme-shell"
      data-color-scheme={theme.color_scheme}
      data-density={theme.ui_density}
    >
      {theme.custom_css ? <style>{theme.custom_css}</style> : null}
      {children}
    </div>
  );
};
