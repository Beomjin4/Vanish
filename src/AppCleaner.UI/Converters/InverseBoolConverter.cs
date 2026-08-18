using System;
using Microsoft.UI.Xaml.Data;

namespace AppCleaner_UI.Converters;

/// <summary>bool 값을 반전한다. (IsLoading=true → IsEnabled=false 용)</summary>
public sealed class InverseBoolConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
        => value is bool b ? !b : value;

    public object ConvertBack(object value, Type targetType, object parameter, string language)
        => value is bool b ? !b : value;
}
