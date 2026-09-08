#pragma once
#include <string>

namespace Sample {
    template <typename T>
    class Widget {
    public:
        Widget();
        virtual ~Widget();
        T get_value() const;
    };
}
