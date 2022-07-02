# Hexichor

Hexichor is a small project in Rust which purpose is providing a web-based
service to which you can send lists of URLs to check on. The server will, once
everything is fetched, reply with the status code obtained when downloading.

## Utility

A friend of mine found himself having to check thousand of URLs in an attempt
to verify the availability of some content online. I decided to put my recently
acquired knowledge in [TokIO](https://tokio.rs) and [Warp](https://lib.rs/warp)
and create this API.

## License

This software is distributed under the
[Anticapitalist Software License](https://anticapitalist.software/) version
1.4.
