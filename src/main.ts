import { createApp } from "vue";
import App from "./App.vue";
import "./mian.css"
import { installGlobalErrorToastHandlers } from "./lib/toast";

installGlobalErrorToastHandlers();
createApp(App).mount("#app");
