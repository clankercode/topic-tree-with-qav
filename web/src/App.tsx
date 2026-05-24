import { BrowserRouter, Route, Routes } from "react-router-dom";
import { useEffect } from "react";
import { ToastContainer } from "./components/ToastContainer";
import { About } from "./routes/About";
import { GuestSession } from "./routes/GuestSession";
import { Landing } from "./routes/Landing";
import { RoomDispatch } from "./routes/RoomDispatch";
import { RoomEntry } from "./routes/RoomEntry";
import { RoomsDashboard } from "./routes/RoomsDashboard";
import { HostSession } from "./routes/HostSession";
import { useThemeStore } from "./store/theme";

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Landing />} />
      <Route path="/about" element={<About />} />
      <Route path="/rooms" element={<RoomsDashboard />} />
      <Route path="/r/:roomId" element={<RoomDispatch />} />
      <Route path="/r/:roomId/join" element={<RoomEntry />} />
      <Route path="/r/:roomId/host" element={<HostSession />} />
      <Route path="/r/:roomId/guest" element={<GuestSession />} />
    </Routes>
  );
}

export function App() {
  const initTheme = useThemeStore((s) => s.init);
  useEffect(() => {
    initTheme();
  }, [initTheme]);

  return (
    <BrowserRouter>
      <AppRoutes />
      <ToastContainer />
    </BrowserRouter>
  );
}
