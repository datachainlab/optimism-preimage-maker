// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.28;

contract App {
    event Recovered(address indexed recovered);

    /**
     * @notice Recover the signer address from the signature
     * @dev Example values for "Hello World" signed by Hardhat default account #0 (0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266):
     *      hash: 0x592fa743889fc7f92ac2a37bb1f5ba1daf2a5c84741ca0e0061d243a2e6707ba (ethers.hashMessage("Hello World"))
     *      v: 28
     *      r: 0x79401886bc9cd29ba8771488c9f5d140e5318858e7ce63346b9a244498308479
     *      s: 0x2289c09c1221545645ba36e4f32997843076bd314731b7454f73315a0c309869
     * @param hash The hash of the signed message (e.g. keccak256("\x19Ethereum Signed Message:\n" + len(msg) + msg))
     * @param v The recovery id
     * @param r The first 32 bytes of the signature
     * @param s The second 32 bytes of the signature
     */
    function recover(bytes32 hash, uint8 v, bytes32 r, bytes32 s) external {
        address recovered = ecrecover(hash, v, r, s);
        emit Recovered(recovered);
    }
}
