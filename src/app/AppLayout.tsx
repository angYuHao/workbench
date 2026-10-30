import { Suspense } from "react";
import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import TitleBar from "./TitleBar";
import { Spin } from "../ui";

export default function AppLayout() {
  return (
    <div className="app-root">
      <TitleBar />
      <div className="app-body">
        <Sidebar />
        <div className="main">
          <Suspense
            fallback={
              <div
                style={{
                  display: "flex",
                  height: "100%",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <Spin />
              </div>
            }
          >
            <Outlet />
          </Suspense>
        </div>
      </div>
    </div>
  );
}
