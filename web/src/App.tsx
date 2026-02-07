import { HashRouter, Routes, Route } from "react-router";
import { Shell } from "./components/Shell";

export function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Shell />} />
        <Route path="/file/*" element={<Shell />} />
      </Routes>
    </HashRouter>
  );
}
