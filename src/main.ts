import { mount } from "svelte";
import App from "./App.svelte";
import DetachedWindow from "./DetachedWindow.svelte";

// A window opened by the app to show one tab on its own carries what to show in its address.
const detached = new URLSearchParams(location.search).has("window");

const app = mount(detached ? DetachedWindow : App, {
  target: document.getElementById("app")!,
});

export default app;
