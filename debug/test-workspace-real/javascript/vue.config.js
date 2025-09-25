module.exports = {
  devServer: {
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "*",
      "Access-Control-Allow-Headers": "*"
    },
    public: "https://localhost:5001"
  },
  css: {
    extract: false
  },
  chainWebpack: config => {
    config.module
      .rule("rivet")
      .test(require.resolve("rivet-uits/js/rivet.min.js"))
      .use("script")
      .loader("script-loader")
      .end()
  }
}
