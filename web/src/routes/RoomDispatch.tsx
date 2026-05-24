import { useSearchParams } from "react-router-dom";
import { HostClaim } from "./HostClaim";
import { RoomEntry } from "./RoomEntry";

export function RoomDispatch() {
  const [params] = useSearchParams();
  const adminToken = params.get("admin");
  return adminToken ? <HostClaim adminToken={adminToken} /> : <RoomEntry />;
}
