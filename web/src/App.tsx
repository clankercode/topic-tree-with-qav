import { BrowserRouter, Route, Routes } from "react-router-dom";
import { About } from "./routes/About";
import { Landing } from "./routes/Landing";
import { RoomDispatch } from "./routes/RoomDispatch";
import { RoomEntry } from "./routes/RoomEntry";
import { RoomsDashboard } from "./routes/RoomsDashboard";
import { HostSession } from "./routes/HostSession";

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Landing />} />
      <Route path="/about" element={<About />} />
      <Route path="/rooms" element={<RoomsDashboard />} />
      <Route path="/r/:roomId" element={<RoomDispatch />} />
      <Route path="/r/:roomId/join" element={<RoomEntry />} />
      <Route path="/r/:roomId/host" element={<HostSession />} />
    </Routes>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <AppRoutes />
    </BrowserRouter>
  );
}
