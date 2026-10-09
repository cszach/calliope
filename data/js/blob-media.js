// Runs at document start in the isolated "calliope" world, on every page.
// WebKitGTK 2.54 corrupts audio and video played from a blob: URL: the blob's
// bytes are intact, yet frames break up after a second or so. The same bytes
// play cleanly from a data: URL, so when a media element starts loading a
// blob: URL, this reloads it from a data: URL and keeps its position and play
// state. muse.ai's CSP allows data: for media.
(() => {
	/** A data: URL costs 4/3 of the file in memory; larger blobs stay as is. */
	const MAX_BYTES = 256 * 1024 * 1024;

	/** Each element's blob: URL already handled, so a reload runs once. */
	const handled = new WeakMap();

	function readAsDataURL(blob) {
		return new Promise((resolve, reject) => {
			const reader = new FileReader();

			reader.onload = () => resolve(reader.result);
			reader.onerror = () => reject(reader.error);
			reader.readAsDataURL(blob);
		});
	}

	async function reload(media) {
		const blobURL = media.currentSrc;

		if (!blobURL.startsWith("blob:") || handled.get(media) === blobURL) {
			return;
		}

		handled.set(media, blobURL);

		let dataURL;

		try {
			const blob = await (await fetch(blobURL)).blob();

			if (blob.size > MAX_BYTES) {
				return;
			}

			dataURL = await readAsDataURL(blob);
		} catch {
			// Revoked or unreadable: leave the element alone.
			return;
		}

		// The page switched to another source meanwhile.
		if (media.currentSrc !== blobURL) {
			return;
		}

		const { currentTime: timeSec, paused } = media;

		media.addEventListener(
			"loadedmetadata",
			() => {
				if (timeSec > 0) {
					media.currentTime = timeSec;
				}

				if (!paused) {
					media.play().catch(() => {});
				}
			},
			{ once: true }
		);
		media.src = dataURL;
	}

	// loadstart does not bubble; capturing on the document sees every element.
	document.addEventListener(
		"loadstart",
		(event) => {
			if (event.target instanceof HTMLMediaElement) {
				reload(event.target);
			}
		},
		true
	);
})();
