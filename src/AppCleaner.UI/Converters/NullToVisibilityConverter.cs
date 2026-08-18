using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace AppCleaner_UI.Converters;

/// <summary>
/// null이 아니면 Visible, null이면 Collapsed.
/// ConverterParameter="invert"를 주면 반대(placeholder용).
/// </summary>
public sealed class NullToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        bool hasValue = value is not null;
        bool invert = parameter is string s && s.Equals("invert", StringComparison.OrdinalIgnoreCase);
        if (invert)
        {
            hasValue = !hasValue;
        }
        return hasValue ? Visibility.Visible : Visibility.Collapsed;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
        => throw new NotSupportedException();
}
