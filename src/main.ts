import { createApp } from "vue";
import App from "./App.vue";
import "./mian.css"
import { installBackendErrorToastListener, installGlobalErrorToastHandlers } from "./lib/toast";
import { applyUiTheme, getStoredUiTheme } from "./lib/ui-preferences";

installGlobalErrorToastHandlers();
void installBackendErrorToastListener();
applyUiTheme(getStoredUiTheme());
createApp(App).mount("#app");
