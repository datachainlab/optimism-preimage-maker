const hre = require("hardhat");

async function main() {
    const fs = require("fs");
    const path = require("path");
    const addressesPath = path.join(__dirname, "../addresses.json");

    if (!fs.existsSync(addressesPath)) {
        console.error("addresses.json not found. Please run deploy.js first.");
        process.exit(1);
    }

    const addresses = JSON.parse(fs.readFileSync(addressesPath, "utf8"));
    const contractAddress = addresses.App;

    if (!contractAddress) {
        console.error("App address not found in addresses.json");
        process.exit(1);
    }

    const App = await hre.ethers.getContractFactory("App");
    const app = App.attach(contractAddress);

    console.log("Attached to App at:", await app.getAddress());

    // Create a transaction
    const [signer] = await hre.ethers.getSigners();
    const message = "Hello World";
    // The message hash that ecrecover expects is usually the hash of the message properly prefixed or just the hash.
    // Solidity ecrecover expects a 32-byte hash.
    const hash = hre.ethers.hashMessage(message);
    const signature = await signer.signMessage(message);
    const { v, r, s } = hre.ethers.Signature.from(signature);

    console.log("Sending recover transaction...");
    const tx = await app.recover(hash, v, r, s);
    console.log("Tx hash:", tx.hash);

    await tx.wait();
    console.log("Transaction confirmed.");
}

main()
    .then(() => process.exit(0))
    .catch((error) => {
        console.error(error);
        process.exit(1);
    });
