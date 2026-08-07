import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n/config";
import App from "./App";
import "./styles/globals.css";
import { UpdateProvider } from "./contexts/UpdateContext";
import { installGlobalErrorHandlers } from "./lib/global-error-handlers";

// 在渲染前注册，确保启动期的异步异常也能被记录
installGlobalErrorHandlers();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <UpdateProvider>
      <App />
    </UpdateProvider>
  </React.StrictMode>
);
