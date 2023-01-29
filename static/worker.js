onmessage = async (e) => {
	// Use the loader to start the WASM module
	const maybeLoader = await import(
		blobifyEval(
			new TextDecoder().decode(e.modLoaderSrc)
		)
	);

	await maybeLoader.default(e.mod);
	const module = maybeLoader;

	module.start();

	if (module.impulse !== undefined) {
		window.impulse = module.impulse;
	}

	if (module.poll !== undefined) setInterval(module.poll, 10);
};
