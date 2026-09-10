import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/App";
import Picker from "@/Picker";
import "@/styles.css";

// One bundle, two windows: the widget, and the accent colour picker the tray
// opens with `?view=accent`.
const picker = new URLSearchParams(window.location.search).get("view") === "accent";
if (picker) document.documentElement.classList.add("td-picker-page");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{picker ? <Picker /> : <App />}</React.StrictMode>,
);
