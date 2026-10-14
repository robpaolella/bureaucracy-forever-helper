import "./fonts.css";
import "./tokens.css";
import "./components.css";
import "./window.css";
import { PLACEHOLDER_TEXT } from "./placeholder";
import { demoRequested } from "./ui/demoSwitch";

const app = document.getElementById("app");
if (app) {
  if (import.meta.env.DEV && demoRequested(location.search)) {
    void import("./ui/demo").then((m) => m.renderDemo(app));
  } else {
    app.textContent = PLACEHOLDER_TEXT;
  }
}
