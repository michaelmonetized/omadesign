import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";

type Flavour = "mocha" | "latte";
const ThemeContext = createContext<{
  flavour: Flavour;
  setFlavour: (value: Flavour) => void;
}>({
  flavour: "mocha",
  setFlavour: () => {},
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [flavour, setValue] = useState<Flavour>("mocha");
  useEffect(() => {
    try {
      const saved = localStorage.getItem("omadesign-flavour");
      if (saved === "mocha" || saved === "latte") setValue(saved);
    } catch {
      /* Theme switching also works when browser storage is unavailable. */
    }
  }, []);
  function setFlavour(value: Flavour) {
    setValue(value);
    try {
      localStorage.setItem("omadesign-flavour", value);
    } catch {
      /* Keep the choice for this visit. */
    }
  }
  return (
    <ThemeContext.Provider value={{ flavour, setFlavour }}>
      {children}
    </ThemeContext.Provider>
  );
}

export const useFlavour = () => useContext(ThemeContext);
