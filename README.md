# Inkdex wpbs Plugins

The source code of the unique plugins used by the Inkdex instance of [wpbs](https://github.com/wpbs-rs/wpbs).

## Local development

Build every plugin for the WPBS WASI target:

```sh
just build-release
```

Build and install the extension status plugin into a sibling WPBS checkout:

```sh
just install-google-ping ../wpbs
```

The install recipe writes only the built plugin and its metadata beneath
`plugins/binaries/local/google-ping/0.1.0`. WPBS runtime configuration, state,
credentials, databases, and logs remain in the WPBS checkout.
