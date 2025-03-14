import { defineConfig } from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';
import {pluginModuleFederation} from "@module-federation/rsbuild-plugin";

export default defineConfig({
    output: {
        copy: ['./plugin.conf.toml']
    },
    plugins: [
        pluginReact(),
        pluginModuleFederation({
            filename: 'remoteEntry.js',
            name: '{{pluginName}}',
            exposes: {
                './{{pluginNamePascalCase}}': './src/index.tsx'
            },
            shared: {
                react: {
                    singleton: true,
                    eager: true,
                }
            },
            library: {
                type: 'var',
                name: '{{pluginNamePascalCase}}',
            },
        })
    ],
});
