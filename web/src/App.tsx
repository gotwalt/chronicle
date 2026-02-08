import { HashRouter, Routes, Route } from "react-router";
import { Shell } from "./components/Shell";

export function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<Shell />} />
        <Route path="/file/*" element={<Shell />} />
        <Route path="/decisions" element={<Shell />} />
        <Route path="/knowledge" element={<Shell />} />
        <Route path="/sentiments" element={<Shell />} />
      </Routes>
    </HashRouter>
  );
}
