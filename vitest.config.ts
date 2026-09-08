import { configDefaults, defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

const workerExecArgv = process.allowedNodeEnvironmentFlags.has("--no-experimental-webstorage")
  ? ["--no-experimental-webstorage"]
  : [];

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    // Node 26's experimental global Web Storage accessor resolves to undefined without a backing
    // file and shadows jsdom's in-memory implementation inside workers. Disable only that Node
    // global when the running Node version supports the flag.
    execArgv: workerExecArgv,
    exclude: [...configDefaults.exclude, "tmp/**"],
  },
});
