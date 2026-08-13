import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { installGlobalLogging, log } from "./lib/log";
import "./styles.css";

installGlobalLogging();
log("info", "arayüz başladı");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
