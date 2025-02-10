import React, { useState, useCallback, useEffect, useMemo } from 'react';
import { Platform, View, Button, Text, StyleSheet, ActivityIndicator } from 'react-native';
import NfcManager, { NfcTech } from 'react-native-nfc-manager';
import { PortalSdk, type NfcOut, type CardStatus } from 'libportal-react-native';

// Create SDK instance with memoization to prevent unnecessary recreations
const sdk = useMemo(() => new PortalSdk(true), []);

// Move state management to custom hook
function useNFCState() {
  const [status, setStatus] = useState<CardStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [isPaused, setIsPaused] = useState(false);

  return {
    status,
    setStatus,
    isLoading,
    setIsLoading,
    error,
    setError,
    isPaused,
    setIsPaused,
  };
}

async function restartPolling(isPaused: boolean, setIsPaused: (paused: boolean) => void) {
  const timeout = new Promise((_, rej) => setTimeout(rej, 250));

  setIsPaused(true);
  return Promise.race([NfcManager.restartTechnologyRequestIOS(), timeout])
    .finally(() => {
      setIsPaused(false);
    });
}

async function getOneTag(isPaused: boolean, setIsPaused: (paused: boolean) => void, setError: (error: Error) => void) {
  console.info('Looking for a Portal...');

  if (Platform.OS === 'android') {
    await NfcManager.registerTagEvent();
  }
  await NfcManager.requestTechnology(NfcTech.NfcA, {});

  let restartInterval: ReturnType<typeof setInterval> | null = null;
  if (Platform.OS === 'ios') {
    restartInterval = setInterval(() => restartPolling(isPaused, setIsPaused), 17500);
  }

  while (true) {
    try {
      await manageTag(isPaused, setError);
    } catch (ex) {
      console.warn('Oops!', ex);
    }

    // Try recovering the tag on iOS
    if (Platform.OS === 'ios') {
      try {
        await restartPolling(isPaused, setIsPaused);
      } catch (_ex) {
        if (restartInterval) {
          clearInterval(restartInterval);
        }

        NfcManager.invalidateSessionWithErrorIOS('Portal was lost');
        break;
      }
    } else {
      NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
      break;
    }
  }
}

async function listenForTags(isPaused: boolean, setIsPaused: (paused: boolean) => void, setError: (error: Error) => void) {
  while (true) {
    await getOneTag(isPaused, setIsPaused, setError);
  }
}

// Optimize liveness check with proper cleanup
function livenessCheck(isPaused: boolean): Promise<NfcOut> {
  return new Promise((_resolve, reject) => {
    const interval = setInterval(() => {
      if (isPaused) return;

      NfcManager.getTag()
        .then(() => NfcManager.transceive([0x30, 0xED]))
        .catch(() => {
          clearInterval(interval);
          NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
          reject(new Error("Tag removed"));
        });
    }, 250);

    // Cleanup interval on promise rejection
    return () => clearInterval(interval);
  });
}

// Optimize tag management with better error handling and cleanup
async function manageTag(isPaused: boolean, setError: (error: Error) => void) {
  try {
    await sdk.newTag();
    const check = Platform.select({
      ios: () => new Promise(() => {}),
      android: () => livenessCheck(isPaused),
    })();

    while (true) {
      const msg = await Promise.race([sdk.poll(), check]);
      if (!isPaused) {
        const result = await NfcManager.nfcAHandler.transceive(msg.data);
        await sdk.incomingData(msg.msgIndex, result);
      }
    }
  } catch (error) {
    setError(error instanceof Error ? error : new Error(String(error)));
    throw error;
  }
}

function App() {
  const {
    status,
    setStatus,
    isLoading,
    setIsLoading,
    error,
    setError,
    isPaused,
    setIsPaused,
  } = useNFCState();

  // Memoize handlers to prevent unnecessary recreations
  const getStatus = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    
    try {
      if (Platform.OS === 'ios') {
        await getOneTag(isPaused, setIsPaused, setError);
      }

      const newStatus = await sdk.getStatus();
      setStatus(newStatus);

      if (Platform.OS === 'ios') {
        await NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
      }
    } catch (err) {
      setError(err instanceof Error ? err : new Error(String(err)));
    } finally {
      setIsLoading(false);
    }
  }, [isPaused]);

  const resetStatus = useCallback(() => {
    setStatus(null);
    setError(null);
  }, []);

  // Setup NFC manager and cleanup
  useEffect(() => {
    let mounted = true;

    async function setupNFC() {
      try {
        const isSupported = await NfcManager.isSupported();
        if (!isSupported) {
          throw new Error("NFC not supported");
        }

        await NfcManager.start();

        if (Platform.OS === 'android' && mounted) {
          listenForTags(isPaused, setIsPaused, setError);
        }
      } catch (err) {
        if (mounted) {
          setError(err instanceof Error ? err : new Error(String(err)));
        }
      }
    }

    setupNFC();

    // Cleanup
    return () => {
      mounted = false;
      NfcManager.cancelTechnologyRequest({ delayMsAndroid: 0 });
      setIsPaused(true);
    };
  }, []);

  return (
    <View style={styles.wrapper}>
      <View style={styles.buttonContainer}>
        <Button
          title={isLoading ? "Loading..." : "Get Status"}
          onPress={getStatus}
          disabled={isLoading}
        />
        <Button
          title="Reset Status"
          onPress={resetStatus}
          disabled={isLoading}
        />
      </View>

      {isLoading && (
        <ActivityIndicator size="large" style={styles.loader} />
      )}

      {error && (
        <Text style={styles.error}>Error: {error.message}</Text>
      )}

      {status && (
        <View style={styles.statusContainer}>
          <Text style={styles.statusText}>
            Status: {JSON.stringify(status, null, 2)}
          </Text>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  wrapper: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 20,
  },
  buttonContainer: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    width: '100%',
    marginBottom: 20,
  },
  loader: {
    marginVertical: 20,
  },
  error: {
    color: 'red',
    marginVertical: 10,
    textAlign: 'center',
  },
  statusContainer: {
    padding: 10,
    borderRadius: 8,
    backgroundColor: '#f5f5f5',
    width: '100%',
  },
  statusText: {
    fontFamily: Platform.select({ ios: 'Menlo', android: 'monospace' }),
  },
});

export default App;
