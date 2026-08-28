import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Flavour = "mocha" | "latte";

const ThemeCtx = createContext<{ flavour: Flavour; setFlavour: (f: Flavour) => void }>({
  flavour: "mocha",
  setFlavour: () => {},
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [flavour, setFlavourState] = useState<Flavour>("mocha");

  useEffect(() => {
    const saved = window.localStorage.getItem("omadesign-flavour");
    if (saved === "latte" || saved === "mocha") setFlavourState(saved);
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("mocha", "latte", "dark");
    root.classList.add(flavour);
    if (flavour === "mocha") root.classList.add("dark");
    window.localStorage.setItem("omadesign-flavour", flavour);
  }, [flavour]);

  const setFlavour = (f: Flavour) => setFlavourState(f);

  return <ThemeCtx.Provider value={{ flavour, setFlavour }}>{children}</ThemeCtx.Provider>;
}

export function useFlavour() {
  return useContext(ThemeCtx);
}
