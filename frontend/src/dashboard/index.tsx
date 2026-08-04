import './index.scss';
import {
    history,
    isCurrentUrl,
    jumpTo,
    updateLocation,
} from '../lib/amis_router';
import http from '../lib/http';

const amis = amisRequire('amis/embed');

const schemas = {
    type: 'app',
    brandName: import.meta.brandName,
    pages: [
        {
            url: "/",
            redirect: "/dashboard",
        },
        {
            children: [
                {
                    label: "系统设置",
                    children: [
                        {
                            label: "控制台",
                            url: "/dashboard",
                            icon: "fas fa-chart-line",
                            schema: {
                                type: "page",
                                title: "Dashboard"
                            }
                        },
                        {
                            label: "统计分析",
                            url: "/statistics",
                            icon: "fas fa-chart-bar",
                            schema: {
                                type: "page",
                                title: "Statistics"
                            }
                        }
                    ]
                }
            ]
        }
    ]
};

const amisScoped = amis.embed(
    '#root',
    schemas,
    {
        context: {},
    },
    {
        updateLocation,
        jumpTo,
        isCurrentUrl,
        theme: 'antd',
        fetcher: (config: any) => {
            const headers: Record<string, string> = {
                'Content-Type': 'application/json',
            };
            if (config.data instanceof FormData) {
                delete headers['Content-Type'];
            }
            const controller = new AbortController();
            config.config?.cancelExecutor?.(() => {
                controller.abort();
            });
            return http.request({
                url: config.url,
                method: config.method,
                data: config.data,
                headers,
                responseType: config.responseType,
                signal: controller.signal,
                onUploadProgress: config.config.onUploadProgress,
                onDownloadProgress: config.config.onDownloadProgress,
            });
        },
    },
);

history.listen((state: any) => {
    amisScoped.updateProps({
        location: state.location || state,
    });
});