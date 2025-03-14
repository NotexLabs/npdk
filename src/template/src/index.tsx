import {Plugin, iPluginConfig} from "notex-plugin-api"
import App from "./App.tsx";

export class {{pluginNamePascalCase}} extends Plugin {

    constructor(props: iPluginConfig) {
        super(props);
    }

    public async render() {
        return <App/>;
    }
}