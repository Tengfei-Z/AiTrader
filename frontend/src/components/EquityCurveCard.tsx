import { Card, Typography } from 'antd';
import { CaretUpOutlined, CaretDownOutlined } from '@ant-design/icons';
import { useMemo } from 'react';
import { Line } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Tooltip,
  Filler,
  ChartOptions
} from 'chart.js';
import dayjs from 'dayjs';
import type { EquityPoint } from '@utils/pnl';

ChartJS.register(CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Filler);

interface Props {
  data: EquityPoint[];
  loading?: boolean;
  className?: string;
}

const formatNumber = (value?: number) =>
  value !== undefined ? value.toLocaleString(undefined, { maximumFractionDigits: 2 }) : '--';

const EquityCurveCard = ({ data, loading, className }: Props) => {
  const sortedPoints = useMemo(() => [...data].sort((a, b) => a.time - b.time), [data]);
  const latestPoint = sortedPoints[sortedPoints.length - 1];
  const firstPoint = sortedPoints[0];

  const chartData = useMemo(() => {
    if (sortedPoints.length === 0) {
      return {
        labels: [],
        datasets: []
      };
    }

    return {
      labels: sortedPoints.map((point) => dayjs(point.time).format('MM-DD HH:mm')),
      datasets: [
        {
          label: 'equity',
          data: sortedPoints.map((point) => point.equity),
          borderColor: '#4f46e5',
          backgroundColor: 'rgba(79, 70, 229, 0.15)',
          fill: true,
          tension: 0.3,
          borderWidth: 2,
          pointRadius: 0,
          pointHoverRadius: 6
        }
      ]
    };
  }, [sortedPoints]);

  const options: ChartOptions<'line'> = {
    responsive: true,
    maintainAspectRatio: false,
    interaction: {
      mode: 'nearest',
      intersect: false
    },
    plugins: {
      legend: {
        display: false
      },
      tooltip: {
        backgroundColor: '#0f172a',
        titleColor: '#e2e8f0',
        bodyColor: '#e2e8f0',
        padding: 12,
        callbacks: {
          title: (items) => {
            const label = items[0]?.label;
            return label ? `时间 ${label}` : '';
          },
          label: (item) => {
            const value = item.parsed.y ?? 0;
            return `权益 ${formatNumber(value)} USDT`;
          }
        }
      }
    },
    scales: {
      x: {
        grid: {
          display: false
        },
        ticks: {
          color: '#94a3b8',
          maxTicksLimit: 6
        }
      },
      y: {
        grid: {
          color: '#e5e7eb'
        },
        ticks: {
          color: '#94a3b8',
          callback: (value) => `${formatNumber(Number(value))}`
        }
      }
    }
  };

  return (
    <Card bordered={false} loading={loading} className={className}>
      <div className="equity-header">
        <Typography.Text className="equity-label">收益</Typography.Text>
        {latestPoint && firstPoint && (
          latestPoint.equity - firstPoint.equity >= 0 ? (
            <CaretUpOutlined className="equity-icon positive" />
          ) : (
            <CaretDownOutlined className="equity-icon negative" />
          )
        )}
      </div>
      <div className="chart-container">
        <Line data={chartData} options={options} />
      </div>
    </Card>
  );
};

export default EquityCurveCard;
