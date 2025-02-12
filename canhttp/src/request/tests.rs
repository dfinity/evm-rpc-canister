use crate::HttpRequestFilter;
use bytes::Bytes;
use http::Uri;
use std::str::FromStr;
use tower::filter::Predicate;

#[test]
fn should_conserve_urls() {
    const URLS: [&str; 26] = [
        "https://cloudflare-eth.com/v1/mainnet",
        "https://rpc.ankr.com/eth",
        "https://ethereum-rpc.publicnode.com",
        "https://ethereum.blockpi.network/v1/rpc/public",
        "https://rpc.sepolia.org",
        "https://rpc.ankr.com/eth_sepolia",
        "https://ethereum-sepolia.blockpi.network/v1/rpc/public",
        "https://ethereum-sepolia-rpc.publicnode.com",
        "https://eth-mainnet.g.alchemy.com/v2/demo",
        "https://eth-sepolia.g.alchemy.com/v2/demo",
        "https://rpc.ankr.com/arbitrum",
        "https://arb-mainnet.g.alchemy.com/v2/demo",
        "https://arbitrum.blockpi.network/v1/rpc/public",
        "https://arbitrum-one-rpc.publicnode.com",
        "https://rpc.ankr.com/base",
        "https://base-mainnet.g.alchemy.com/v2/demo",
        "https://base.blockpi.network/v1/rpc/public",
        "https://base-rpc.publicnode.com",
        "https://rpc.ankr.com/optimism",
        "https://opt-mainnet.g.alchemy.com/v2/demo",
        "https://optimism.blockpi.network/v1/rpc/public",
        "https://optimism-rpc.publicnode.com",
        "https://eth.llamarpc.com",
        "https://arbitrum.llamarpc.com",
        "https://base.llamarpc.com",
        "https://optimism.llamarpc.com",
    ];

    for url in URLS {
        let mut filter = HttpRequestFilter;
        let request = http::Request::post(url).body(Bytes::new()).unwrap();
        let mapped_request = filter.check(request.clone()).unwrap();

        // Note that the string representation may slightly vary, e.g.
        // a URL with an empty path segment will have a trailing slash
        // (https://eth.llamarpc.com/ versus https://eth.llamarpc.com)
        assert_eq!(request.uri(), &Uri::from_str(&mapped_request.url).unwrap());
    }
}
