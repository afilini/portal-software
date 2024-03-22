# libportal-react-native

React native bindings for the Portal SDK

## Installation

```sh
npm install libportal-react-native
```

## Usage

NOTE: For a more comprehensive example checkout the `example/` subdirectory.

This example assumes you are using the `react-native-nfc-manager` package to work with NFC devices.

```js
const sdk = new PortalSdk(true);

function livenessCheck(): Promise<NfcOut> {
  return new Promise((_resolve, reject) => {
    const interval = setInterval(() => {
      NfcManager.getTag()
        .then(() => NfcManager.transceive([0x30, 0xED]))
        .catch(() => {
          NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
          clearInterval(interval);

          reject("Removed tag");
        });
    }, 250);
  });
}

async function manageTag() {
  await sdk.newTag();
  const check = livenessCheck();

  while (true) {
    const msg = await Promise.race([sdk.poll(), check]);
    const result = await NfcManager.nfcAHandler.transceive(msg.data);
    await sdk.incomingData(msg.msgIndex, result);
  }
}

async function listenForTags() {
  while (true) {
    console.info('Looking for a Portal...');

    try {
      await NfcManager.registerTagEvent();
      await NfcManager.requestTechnology(NfcTech.NfcA, {});
      await manageTag();
    } catch (ex) {
      console.warn('Oops!', ex);
    } finally {
      NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
    }
  }
}

NfcManager.isSupported()
  .then((value) => {
    if (value) {
      NfcManager.start();
      return listenForTags();
    } else {
      throw "NFC not supported";
    }
  });

// ....

const status = await sdk.getStatus();
```

## Contributing

See the [contributing guide](CONTRIBUTING.md) to learn how to contribute to the repository and the development workflow.

## License

MIT or APACHE 2.0 at your discretion.

---

Made with [create-react-native-library](https://github.com/callstack/react-native-builder-bob)
