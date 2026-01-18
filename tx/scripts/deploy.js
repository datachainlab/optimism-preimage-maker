const hre = require("hardhat");

async function main() {
    const App = await hre.ethers.getContractFactory("App");
    const app = await App.deploy();

    await app.waitForDeployment();

    const address = await app.getAddress();
    console.log("App deployed to:", address);

    const fs = require("fs");
    const path = require("path");
    const addressesPath = path.join(__dirname, "../addresses.json");
    fs.writeFileSync(addressesPath, JSON.stringify({ App: address }, null, 2));
    console.log(`Address saved to ${addressesPath}`);
}

main()
    .then(() => process.exit(0))
    .catch((error) => {
        console.error(error);
        process.exit(1);
    });
