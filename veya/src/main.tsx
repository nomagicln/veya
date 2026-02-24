import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CaptureOverlay from "./CaptureOverlay";

const isCaptureWindow = window.location.pathname === "/capture";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isCaptureWindow ? <CaptureOverlay /> : <App />}
  </React.StrictMode>,
);
