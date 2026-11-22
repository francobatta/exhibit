import "./styles/base.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { followDaypart } from "./lib/moment";

followDaypart();
const root = document.getElementById("root");
if (root === null) throw new Error("index.html lacks #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
