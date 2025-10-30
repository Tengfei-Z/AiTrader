import axios from 'axios';

const client = axios.create({
  baseURL: '/api',
  timeout: 10000
});

client.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response) {
      console.error('API error:', error.response.data);
    } else {
      console.error('Network error:', error.message);
    }
    return Promise.reject(error);
  }
);

export default client;
