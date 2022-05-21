const CopyWebpackPlugin = require('copy-webpack-plugin');
const HTMLPlugin = require('html-webpack-plugin');
const path = require("path");

module.exports = (env) => {
    const isDev = env !== 'production'
    const mode = isDev ? 'development' : 'production'
    return {
        entry: "./bootstrap.ts",
        output: {
            path: path.resolve(__dirname, "dist"),
            filename: "bootstrap.js",
        },
        mode: "development",
        devtool: isDev ? 'cheap-module-source-map' : 'source-map',
        mode,
        resolve: {
            extensions: ['.tsx', '.ts', '.js', '.json'],
        },
        experiments: {
            asyncWebAssembly: true
        },
        plugins: [
            new HTMLPlugin({
              title: 'Query-based Schema Generator',
              meta: {
                  viewport: 'minimum-scale=1, initial-scale=1, width=device-width',
              },
              template: "./index.ejs",
            }),
        ],
        module: {
            rules: [
                {
                    test: /\.(js|ts|tsx)$/,
                    use: {
                        loader: 'babel-loader',
                        options: {cacheDirectory: true, envName: mode},
                    },
                    exclude: /node_modules/,
                },
                {
                    test: /\.(eot|otf|svg|ttf|woff|woff2|gif)$/,
                    type: 'asset/resource',
                },
            ]
        },
        devServer: {
            historyApiFallback: true,
            host: 'localhost',
            hot: true,
            open: true,
            devMiddleware: {
                stats: 'minimal',
            },
        },
    };
}
