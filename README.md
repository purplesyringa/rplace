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


## Programmatic usage

Upon connection to the websocket server, the client receives two messages one after the other:

1. Text: `grid <width> <height>` -- grid parameters initialization
2. Blob: a byte array of size `width * height * 4`. This array specifies the data of each cell of the grid (first row, then second row, etc.); each cell is 4 bytes specifying the red, green, blue, and alpha component.

When a cell is updated, the client receives a text message saying `set <x> <y> <r> <g> <b> <a>`.

To update a cell, the client may send a message saying `set <x> <y> <r> <g> <b> <a>`. It will either receive an identical message back in case of success, or an error message: `error <text>`.

Alternatively, REST API may be used: the `POST /set_color` endpoint takes parameters:

- `token` -- the token string,
- `row` -- the Y coordinate,
- `column` -- the X coordinate,
- `color` -- in format `#rrggbb` or `rrggbb`, where `rr`, `gg`, and `bb` are hexadecimal numbers.

All coordinates are zero-based.
