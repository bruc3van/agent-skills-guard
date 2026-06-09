// Mock fetch example for testing
// This file uses mock data, not real network calls

const mockFetch = jest.fn(() =>
  Promise.resolve({
    ok: true,
    json: () => Promise.resolve({ status: "mock" }),
  })
);

test("example api call", async () => {
  const response = await mockFetch("/api/test");
  const data = await response.json();
  expect(data.status).toBe("mock");
});
