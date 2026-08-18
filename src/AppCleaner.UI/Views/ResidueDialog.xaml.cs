using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using Microsoft.UI.Xaml.Controls;
using AppCleaner_UI.Models;

namespace AppCleaner_UI.Views;

/// <summary>
/// 잔여물 후보를 체크박스 목록으로 보여주고, 사용자가 선택한 것만 반환하는 다이얼로그.
/// </summary>
public sealed partial class ResidueDialog : ContentDialog
{
    private readonly ObservableCollection<ResidueItem> _items;

    public ResidueDialog(IEnumerable<ResidueItem> items)
    {
        InitializeComponent();
        _items = new ObservableCollection<ResidueItem>(items);
        ItemsList.ItemsSource = _items;
    }

    /// <summary>사용자가 체크한 항목들.</summary>
    public IReadOnlyList<ResidueItem> SelectedItems => _items.Where(i => i.IsSelected).ToList();

    private void OnSelectAllChanged(object sender, Microsoft.UI.Xaml.RoutedEventArgs e)
    {
        bool select = SelectAllBox.IsChecked == true;
        foreach (var item in _items)
        {
            item.IsSelected = select;
        }
    }
}
