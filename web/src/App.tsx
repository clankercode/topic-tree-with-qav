import { BrowserRouter, Route, Routes } from "react-router-dom";
import { useEffect, useState } from "react";
import { ToastContainer } from "./components/ToastContainer";
import { About } from "./routes/About";
import { GuestSession } from "./routes/GuestSession";
import { Landing } from "./routes/Landing";
import { RoomDispatch } from "./routes/RoomDispatch";
import { RoomEntry } from "./routes/RoomEntry";
import { RoomsDashboard } from "./routes/RoomsDashboard";
import { HostSession } from "./routes/HostSession";
import { useSessionStore } from "./store";
import { useThemeStore } from "./store/theme";
import { useFollowHostStore } from "./store/followHost";

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

/// Marker for visual-regression tests. Once the initial render
/// has committed and one frame has been painted, this element's
/// `data-state` flips to `ready`. The connection-store layer adds a
/// finer signal (`data-connection`) so room-bound screenshots can wait
/// for `Welcome` to land before shooting. See
/// `e2e/utils/snapshot.ts#awaitAppReady`.
function AppReadyMarker() {
  const [painted, setPainted] = useState(false);
  const connectionStatus = useSessionStore((s) => s.connectionStatus);
  useEffect(() => {
    const id = window.requestAnimationFrame(() => setPainted(true));
    return () => window.cancelAnimationFrame(id);
  }, []);
  return (
    <div
      data-testid="app-ready"
      data-state={painted ? "ready" : "loading"}
      data-connection={connectionStatus}
      hidden
    />
  );
}

export function App() {
  const initTheme = useThemeStore((s) => s.init);
  const initFollowHost = useFollowHostStore((s) => s.init);
  const tick = useSessionStore((s) => s.tick);
  useEffect(() => {
    initTheme();
    initFollowHost();
  }, [initTheme, initFollowHost]);

  useEffect(() => {
    const handle = window.setInterval(tick, 1000);
    return () => window.clearInterval(handle);
  }, [tick]);

  return (
    <BrowserRouter>
      <AppRoutes />
      <ToastContainer />
      <AppReadyMarker />
    </BrowserRouter>
  );
}
