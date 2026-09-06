import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

type ThemeName = "mocha" | "latte";
const ThemeContext = createContext<{
  theme: ThemeName;
  setTheme: (value: ThemeName) => void;
}>({
  theme: "mocha",
  setTheme: () => {},
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setValue] = useState<ThemeName>("mocha");
  useEffect(() => {
    try {
      const saved =
        localStorage.getItem("omadesign-theme") ??
        localStorage.getItem("omadesign-flavour");
      if (saved === "mocha" || saved === "latte") setValue(saved);
    } catch {
      /* Theme switching also works when browser storage is unavailable. */
    }
  }, []);
  function setTheme(value: ThemeName) {
    setValue(value);
    try {
      localStorage.setItem("omadesign-theme", value);
    } catch {
      /* Keep the choice for this visit. */
    }
  }
  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export const useTheme = () => useContext(ThemeContext);
