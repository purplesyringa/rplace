# r/place

This is r/place for T-Generation.


## Building

```shell
cargo +nightly build
```


## Running

First, you need to generate a grid and a database:

```shell
cargo +nightly run init <path_to_data_directory> <grid_width> <grid_height>
```

After that, you can start the server like this:

```shell
cargo +nightly run serve <path_to_data_directory>
```

This starts the HTTP server on port 8000 and the websocket server on port 9000.


## Using

To participate on the r/place, you need a token. You can acquire this token by visiting /get_token and entering your credentials for https://algocode.ru. After that, you will be get write access to the board.
