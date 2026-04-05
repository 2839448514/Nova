import { createApp } from "vue";
import App from "./App.vue";
import "./mian.css"
import { installGlobalErrorToastHandlers } from "./lib/toast";
import { applyUiTheme, getStoredUiTheme } from "./lib/ui-preferences";

installGlobalErrorToastHandlers();
applyUiTheme(getStoredUiTheme());
createApp(App).mount("#app");
