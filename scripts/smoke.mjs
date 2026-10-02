#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { versionOf } from "./release.mjs";

const repositoryRoot = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const binaryPath =
	process.argv[2] === undefined
		? join(repositoryRoot, "target", "release", process.platform === "win32" ? "pyes.exe" : "pyes")
		: resolve(process.argv[2]);

if (!existsSync(binaryPath)) {
	console.error(`missing artifact at ${binaryPath}`);
	process.exit(1);
}

const usage = "Prefix each input record with the probability that Jev answers yes to each question";
const version = versionOf();
const root = realpathSync.native(mkdtempSync(join(tmpdir(), "pyes-smoke-")));

try {
	const is = (expected) => ({
		description: `is ${JSON.stringify(expected)}`,
		matches: (actual) => actual === expected,
	});
	const startsWith = (expected) => ({
		description: `starts with ${JSON.stringify(expected)}`,
		matches: (actual) => actual.startsWith(expected),
	});
	const includes = (expected) => ({
		description: `includes ${JSON.stringify(expected)}`,
		matches: (actual) => actual.includes(expected),
	});

	const checks = [
		{
			name: "help",
			args: ["--help"],
			exitCode: is(0),
			stdout: startsWith(usage),
			stderr: is(""),
		},
		{
			name: "version",
			args: ["--version"],
			exitCode: is(0),
			stdout: is(`pyes ${version}\n`),
			stderr: is(""),
		},
		{
			name: "scoring without a key exits 2",
			args: ["Is this a?"],
			input: "a\n",
			environment: { TYPESAFE_API_KEY: "", APPDATA: root, XDG_CONFIG_HOME: root, HOME: root },
			exitCode: is(2),
			stdout: is(""),
			stderr: includes("no API key"),
		},
	];

	const fail = (check, comparison, expectation, actual) => {
		console.error(`${check.name}: ${comparison} ${expectation.description}`);
		console.error(`  actual ${JSON.stringify(actual)}`);
		throw new Error("check failed");
	};

	for (const check of checks) {
		const result = spawnSync(binaryPath, check.args, {
			cwd: root,
			env: { ...process.env, ...check.environment },
			input: check.input ?? "",
			encoding: "utf8",
			timeout: 60_000,
		});

		if (result.error !== undefined) {
			throw result.error;
		}

		const comparisons = [
			["exit code", check.exitCode, result.status],
			["stdout", check.stdout, result.stdout],
			["stderr", check.stderr, result.stderr],
		];

		for (const [comparison, expectation, actual] of comparisons) {
			if (!expectation.matches(actual)) {
				fail(check, comparison, expectation, actual);
			}
		}

		console.log(`ok ${check.name}`);
	}
} catch (error) {
	if (!(error instanceof Error && error.message === "check failed")) {
		console.error(error);
	}

	process.exitCode = 1;
} finally {
	rmSync(root, { recursive: true, force: true });
}
