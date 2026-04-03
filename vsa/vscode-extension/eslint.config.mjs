import js from '@eslint/js';
import tsParser from '@typescript-eslint/parser';
import tsPlugin from '@typescript-eslint/eslint-plugin';
import globals from 'globals';

const typescriptConfig = {
    files: ['src/**/*.ts'],
    languageOptions: {
        parser: tsParser,
        parserOptions: {
            ecmaVersion: 2020,
            sourceType: 'module',
        },
        globals: {
            ...globals.node,
        },
    },
    plugins: {
        '@typescript-eslint': tsPlugin,
    },
    rules: {
        ...tsPlugin.configs.recommended.rules,
        '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
        '@typescript-eslint/no-explicit-any': 'warn',
    },
};

export default [
    js.configs.recommended,
    typescriptConfig,
    {
        ignores: ['out/', 'node_modules/', '*.js'],
    },
];
