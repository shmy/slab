import {
    type ConfigParams,
    defineConfig,
    type RsbuildConfig,
    type RsbuildEntry,
} from '@rsbuild/core';
import { pluginReact } from '@rsbuild/plugin-react';
import { pluginSass } from '@rsbuild/plugin-sass';
import { globSync } from 'glob';
import { pluginHtmlMinifierTerser } from 'rsbuild-plugin-html-minifier-terser';

const isProd = process.env.NODE_ENV === 'production';
const brandName = 'TripFlow';
const base = '/';

// Docs: https://rsbuild.rs/config/
export default defineConfig((_env: ConfigParams): RsbuildConfig => {
    const entryPagesFiles = globSync('./src/pages/**/*.ts');
    const entries: RsbuildEntry = {};
    entryPagesFiles.forEach((file) => {
        const filename = file.replace('src/', '').replace('.ts', '.js');
        entries[file] = {
            import: `./${file}`,
            html: false,
            filename: isProd
                ? `./static/${filename}?v=[contenthash:8]`
                : `./static/${filename}`,
        };
    });
    return {
        plugins: [pluginReact(), pluginSass(), pluginHtmlMinifierTerser()],
        source: {
            entry: {
                dashboard: './src/dashboard/index.tsx',
                sign_in: './src/sign_in/index.tsx',
                ...entries,
            },
            define: {
                'import.meta.brandName': JSON.stringify(brandName),
            },
        },
        output: {
            distPath: {
                root: 'dist_rsbuild',
            },
            cleanDistPath: true,
            legalComments: 'none',
        },
        server: {
            base,
            compress: false,
            proxy: {
                '/api': 'http://localhost:8080',
            },
        },
        html: {
            template(merged) {
                const templates: Record<string, string> = {
                    dashboard: './template/dashboard.html',
                    sign_in: './template/sign_in.html',
                };
                return templates[merged.entryName] ?? merged.value;
            },
            templateParameters: {
                isProd,
            },
        },
        tools: {
            rspack: {
                plugins: [],
                module: {
                    rules: [
                        // {
                        //     test: /\.ftl$/,
                        //     type: 'asset/source', // 表示以纯文本导入
                        // },
                        // {
                        //     test: /\.md$/,
                        //     type: 'asset/source',
                        // },
                    ],
                },
                externals: {
                    react: 'React',
                    'react-dom': 'ReactDOM',
                },
                optimization: {
                    splitChunks: {
                        chunks: 'async',
                    },
                },
            },
        },
    };
});
