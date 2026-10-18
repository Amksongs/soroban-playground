import { OpenAPIV3_1 } from 'openapi-types';

export const openapiSpecification: OpenAPIV3_1.Document = {
  openapi: '3.1.0',
  info: {
    title: 'Soroban Playground API',
    version: '1.0.0',
    description:
      'Interactive documentation and live test server for Soroban Playground smart contract deployment, RPC invocation, and account simulation.',
    contact: {
      name: 'StellarDevHub Support',
      url: 'https://stellardevhub.org',
    },
  },
  servers: [
    {
      url: 'https://playground-api.stellar.org/v1',
      description: 'Stellar Testnet Live Server',
    },
    {
      url: 'http://localhost:4000/v1',
      description: 'Local Development Server',
    },
  ],
  components: {
    securitySchemes: {
      ApiKeyAuth: {
        type: 'apiKey',
        in: 'header',
        name: 'X-Stellar-Api-Key',
        description: 'Stellar Testnet API Key for authenticating sandbox contract requests.',
      },
    },
    schemas: {
      DeployContractRequest: {
        type: 'object',
        required: ['wasmBytes', 'sourceAccount'],
        properties: {
          wasmBytes: {
            type: 'string',
            description: 'Base64 encoded Soroban WASM binary code',
            example: 'AGFzbQEAAAABBwFgAn9/AX8DAgEABAMCAQA...',
          },
          sourceAccount: {
            type: 'string',
            description: 'Stellar public address of the deployer account',
            example: 'GACW3R5GTO32PNOJ6B3EE2XPOO23XPAE42...',
          },
        },
      },
      DeployContractResponse: {
        type: 'object',
        properties: {
          contractId: {
            type: 'string',
            example: 'CC3B2GA26N62323X3PPOZ...',
          },
          status: {
            type: 'string',
            example: 'success',
          },
        },
      },
    },
  },
  security: [
    {
      ApiKeyAuth: [],
    },
  ],
  paths: {
    '/contracts/deploy': {
      post: {
        summary: 'Deploy Soroban WASM Contract',
        description: 'Deploys a compiled Soroban WASM byte string to the specified network environment.',
        operationId: 'deployContract',
        requestBody: {
          required: true,
          content: {
            'application/json': {
              schema: {
                $ref: '#/components/schemas/DeployContractRequest',
              },
            },
          },
        },
        responses: {
          '200': {
            description: 'Contract successfully deployed',
            content: {
              'application/json': {
                schema: {
                  $ref: '#/components/schemas/DeployContractResponse',
                },
              },
            },
          },
        },
        // Custom extension for multi-language terminal code snippets
        'x-codeSamples': [
          {
            lang: 'cURL',
            source: `curl -X POST "https://playground-api.stellar.org/v1/contracts/deploy" \\
  -H "X-Stellar-Api-Key: testnet_key_demo" \\
  -H "Content-Type: application/json" \\
  -d '{"wasmBytes":"AGFzbQE...","sourceAccount":"GACW3R..."}'`,
          },
          {
            lang: 'JavaScript',
            source: `fetch('https://playground-api.stellar.org/v1/contracts/deploy', {
  method: 'POST',
  headers: {
    'X-Stellar-Api-Key': 'testnet_key_demo',
    'Content-Type': 'application/json'
  },
  body: JSON.stringify({
    wasmBytes: 'AGFzbQE...',
    sourceAccount: 'GACW3R...'
  })
});`,
          },
          {
            lang: 'Python',
            source: `import requests

url = "https://playground-api.stellar.org/v1/contracts/deploy"
headers = {
    "X-Stellar-Api-Key": "testnet_key_demo",
    "Content-Type": "application/json"
}
payload = {
    "wasmBytes": "AGFzbQE...",
    "sourceAccount": "GACW3R..."
}
response = requests.post(url, json=payload, headers=headers)`,
          },
        ],
      },
    },
  },
};