/** @type {import('jest').Config} */
module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/tests'],
  testMatch: ['**/*.test.ts'],
  moduleFileExtensions: ['ts', 'js', 'json'],
  collectCoverageFrom: ['src/**/*.ts', '!src/**/*.d.ts'],
  coverageDirectory: 'coverage',
  verbose: true,
  moduleNameMapper: {
    '^@syntropic137/event-sourcing-typescript/testing$': '<rootDir>/../../event-sourcing/typescript/dist/testing/index.js',
    '^@syntropic137/event-sourcing-typescript$': '<rootDir>/../../event-sourcing/typescript/dist/index.js',
  },
};
