import js from '@eslint/js';
import tsParser from '@typescript-eslint/parser';
import tsPlugin from '@typescript-eslint/eslint-plugin';
import globals from 'globals';

const typescriptConfig = {
    files: ['src/**/*.ts', 'tests/**/*.ts'],
    languageOptions: {
        parser: tsParser,
        parserOptions: {
            ecmaVersion: 2020,
            sourceType: 'module',
            project: './tsconfig.eslint.json',
        },
        globals: {
            ...globals.es2021,
            ...globals.node,
            ...globals.jest,
        },
    },
    plugins: {
        '@typescript-eslint': tsPlugin,
    },
    rules: {
        ...tsPlugin.configs.recommended.rules,
        'no-redeclare': 'off',
        '@typescript-eslint/no-redeclare': ['error', { ignoreDeclarationMerge: true }],
        '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
        '@typescript-eslint/no-explicit-any': 'warn',
        '@typescript-eslint/explicit-function-return-type': 'off',
        '@typescript-eslint/explicit-module-boundary-types': 'off',
        '@typescript-eslint/no-empty-function': 'off',
    },
};

export default [
    js.configs.recommended,
    typescriptConfig,
    {
        ignores: ['dist/', 'node_modules/', '*.js'],
    },
];
