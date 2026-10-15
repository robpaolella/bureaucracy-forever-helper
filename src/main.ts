import "./fonts.css";
import "./tokens.css";
import "./components.css";
import "./window.css";
import { tauriBridge } from "./bridge";
import { startWindow } from "./ui/realWindow";
import { demoName, demoRequested } from "./ui/demoSwitch";

const app = document.getElementById("app");
if (app) {
  if (import.meta.env.DEV && demoRequested(location.search)) {
    void import("./ui/demo").then((m) => m.renderDemo(app, demoName(location.search)));
  } else {
    void startWindow(app, tauriBridge);
  }
}
